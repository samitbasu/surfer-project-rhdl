use std::sync::{atomic::AtomicU64, Arc};

use crate::wasm_util::{perform_async_work, perform_work};
use camino::Utf8PathBuf;
use color_eyre::eyre::{anyhow, WrapErr};
use color_eyre::Result;
use eframe::egui::{self, DroppedFile};
use futures_util::FutureExt;
use futures_util::TryFutureExt;
use log::{error, info};
use rfd::AsyncFileDialog;
use serde::{Deserialize, Serialize};

use crate::{message::Message, wave_container::WaveContainer, State};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum WaveSource {
    File(Utf8PathBuf),
    Data,
    DragAndDrop(Option<Utf8PathBuf>),
    Url(String),
}

pub fn string_to_wavesource(path: String) -> WaveSource {
    if path.starts_with("https://") || path.starts_with("http://") {
        WaveSource::Url(path)
    } else {
        WaveSource::File(path.into())
    }
}

impl std::fmt::Display for WaveSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaveSource::File(file) => write!(f, "{file}"),
            WaveSource::Data => write!(f, "File data"),
            WaveSource::DragAndDrop(None) => write!(f, "Dropped file"),
            WaveSource::DragAndDrop(Some(filename)) => write!(f, "Dropped file ({filename})"),
            WaveSource::Url(url) => write!(f, "{url}"),
        }
    }
}

#[derive(Debug)]
pub struct LoadOptions {
    pub keep_signals: bool,
    pub keep_unavailable: bool,
}

impl LoadOptions {
    pub fn clean() -> LoadOptions {
        LoadOptions {
            keep_signals: false,
            keep_unavailable: false,
        }
    }
}

#[derive(Debug)]
pub enum OpenMode {
    Open,
    Switch,
}

pub enum LoadProgress {
    Downloading(String),
    Loading(Option<u64>, Arc<AtomicU64>),
}

const WELLEN_SURFER_DEFAULT_OPTIONS: wellen::vcd::LoadOptions = wellen::vcd::LoadOptions {
    multi_thread: true,
    remove_scopes_with_empty_name: true,
};

impl State {
    pub fn load_vcd_from_file(
        &mut self,
        vcd_filename: Utf8PathBuf,
        load_options: LoadOptions,
    ) -> Result<()> {
        info!("Load VCD: {vcd_filename}");
        let source = WaveSource::File(vcd_filename.clone());
        let sender = self.sys.channels.msg_sender.clone();

        perform_work(move || {
            let result = wellen::vcd::read_with_options(
                vcd_filename.as_str(),
                WELLEN_SURFER_DEFAULT_OPTIONS,
            )
            .map_err(|e| anyhow!("{e:?}"))
            .with_context(|| format!("Failed to parse VCD file: {source}"));

            match result {
                Ok(waves) => sender
                    .send(Message::WavesLoaded(
                        source,
                        Box::new(WaveContainer::new_waveform(waves)),
                        load_options,
                    ))
                    .unwrap(),
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });

        Ok(())
    }

    pub fn load_vcd_from_data(
        &mut self,
        vcd_data: Vec<u8>,
        load_options: LoadOptions,
    ) -> Result<()> {
        let total_bytes = vcd_data.len();

        self.load_vcd_from_bytes(
            WaveSource::Data,
            vcd_data,
            Some(total_bytes as u64),
            load_options,
        );
        Ok(())
    }

    pub fn load_vcd_from_dropped(&mut self, file: DroppedFile) -> Result<()> {
        info!("Got a dropped file");

        let filename = file.path.and_then(|p| Utf8PathBuf::try_from(p).ok());
        let bytes = file
            .bytes
            .ok_or_else(|| anyhow!("Dropped a file with no bytes"))?;

        let total_bytes = bytes.len();

        self.load_vcd_from_bytes(
            WaveSource::DragAndDrop(filename),
            bytes.to_vec(),
            Some(total_bytes as u64),
            LoadOptions::clean(),
        );
        Ok(())
    }

    pub fn load_vcd_from_url(&mut self, url: String, load_options: LoadOptions) {
        let sender = self.sys.channels.msg_sender.clone();
        let url_ = url.clone();
        let task = async move {
            let bytes = reqwest::get(&url)
                .map(|e| e.with_context(|| format!("Failed fetch download {url}")))
                .and_then(|resp| {
                    resp.bytes()
                        .map(|e| e.with_context(|| format!("Failed to download {url}")))
                })
                .await;

            match bytes {
                Ok(b) => sender.send(Message::FileDownloaded(url, b, load_options)),
                Err(e) => sender.send(Message::Error(e)),
            }
            .unwrap();
        };
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);

        self.sys.vcd_progress = Some(LoadProgress::Downloading(url_))
    }

    pub fn load_vcd_from_bytes(
        &mut self,
        source: WaveSource,
        bytes: Vec<u8>,
        total_bytes: Option<u64>,
        load_options: LoadOptions,
    ) {
        // Progress tracking in bytes
        let progress_bytes = Arc::new(AtomicU64::new(0));

        let sender = self.sys.channels.msg_sender.clone();

        perform_work(move || {
            let result =
                wellen::vcd::read_from_bytes_with_options(&bytes, WELLEN_SURFER_DEFAULT_OPTIONS)
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| format!("Failed to parse VCD file: {source}"));

            match result {
                Ok(waves) => sender
                    .send(Message::WavesLoaded(
                        source,
                        Box::new(WaveContainer::new_waveform(waves)),
                        load_options,
                    ))
                    .unwrap(),
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });

        info!("Setting VCD progress");
        self.sys.vcd_progress = Some(LoadProgress::Loading(total_bytes, progress_bytes));
    }

    pub fn open_file_dialog(&mut self, mode: OpenMode) {
        let sender = self.sys.channels.msg_sender.clone();
        let keep_unavailable = self.config.behavior.keep_during_reload;

        perform_async_work(async move {
            if let Some(file) = AsyncFileDialog::new()
                .set_title("Open waveform file")
                .add_filter("VCD-files (*.vcd)", &["vcd"])
                .add_filter("All files", &["*"])
                .pick_file()
                .await
            {
                let keep_signals = match mode {
                    OpenMode::Open => false,
                    OpenMode::Switch => true,
                };

                #[cfg(not(target_arch = "wasm32"))]
                sender
                    .send(Message::LoadVcd(
                        camino::Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap(),
                        LoadOptions {
                            keep_signals,
                            keep_unavailable,
                        },
                    ))
                    .unwrap();

                #[cfg(target_arch = "wasm32")]
                {
                    let data = file.read().await;
                    sender
                        .send(Message::LoadVcdFromData(
                            data,
                            LoadOptions {
                                keep_signals,
                                keep_unavailable,
                            },
                        ))
                        .unwrap();
                }
            }
        });
    }

    pub fn open_state_save_dialog(&mut self) {
        let Some(encoded) = self.encode_state() else {
            return;
        };

        perform_async_work(async move {
            if let Some(write_dest) = AsyncFileDialog::new()
                .set_title("Save state")
                .add_filter("Surfer state files (*.ron)", &["ron"])
                .add_filter("All files", &["*"])
                .save_file()
                .await
            {
                write_dest
                    .write(encoded.as_bytes())
                    .await
                    .map_err(|e| error!("Failed to write state. {e:#?}"))
                    .ok();
            }
        });
    }
}

pub fn draw_progress_panel(ctx: &egui::Context, vcd_progress_data: &LoadProgress) {
    egui::TopBottomPanel::top("progress panel").show(ctx, |ui| {
        ui.vertical_centered_justified(|ui| match vcd_progress_data {
            LoadProgress::Downloading(url) => {
                ui.spinner();
                ui.monospace(format!("Downloading {url}"));
            }
            LoadProgress::Loading(total_bytes, bytes_done) => {
                let num_bytes = bytes_done.load(std::sync::atomic::Ordering::Relaxed);

                if let Some(total) = total_bytes {
                    ui.monospace(format!("Loading. {num_bytes}/{total} bytes loaded"));
                    let progress = num_bytes as f32 / *total as f32;
                    let progress_bar = egui::ProgressBar::new(progress)
                        .show_percentage()
                        .desired_width(300.);

                    ui.add(progress_bar);
                } else {
                    ui.spinner();
                    ui.monospace(format!("Loading. {num_bytes} bytes loaded"));
                };
            }
        });
    });
}
