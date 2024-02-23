use std::fmt::{Display, Formatter};
use std::sync::{atomic::AtomicU64, Arc};

use crate::wasm_util::{perform_async_work, perform_work};
use camino::Utf8PathBuf;
use color_eyre::eyre::{anyhow, bail, WrapErr};
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

impl Display for WaveSource {
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

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum WaveFormat {
    Vcd,
    Fst,
    Ghw,
}

impl Display for WaveFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            WaveFormat::Vcd => write!(f, "VCD"),
            WaveFormat::Fst => write!(f, "FST"),
            WaveFormat::Ghw => write!(f, "GHW"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LoadOptions {
    pub keep_variables: bool,
    pub keep_unavailable: bool,
    /// Auto-detect if None, otherwise, we error when we get the wrong format.
    pub expect_format: Option<WaveFormat>,
}

impl LoadOptions {
    pub fn clean() -> Self {
        Self {
            keep_variables: false,
            keep_unavailable: false,
            expect_format: None,
        }
    }

    pub fn clean_with_expected_format(format: WaveFormat) -> Self {
        Self {
            keep_variables: false,
            keep_unavailable: false,
            expect_format: Some(format),
        }
    }
}

#[derive(Debug, Deserialize)]
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

fn check_format(
    load_options: &LoadOptions,
    detected_format: wellen::FileFormat,
    source: &WaveSource,
) -> Result<WaveFormat> {
    // check format restrictions
    match load_options.expect_format {
        Some(WaveFormat::Vcd) if detected_format != wellen::FileFormat::Vcd => {
            bail!("{} does not appear to be a VCD file.", source)
        }
        Some(WaveFormat::Fst) if detected_format != wellen::FileFormat::Fst => {
            bail!("{} does not appear to be a FST file.", source)
        }
        _ => {} // OK
    }

    // convert detected file type to Surfer type
    match detected_format {
        wellen::FileFormat::Vcd => Ok(WaveFormat::Vcd),
        wellen::FileFormat::Fst => Ok(WaveFormat::Fst),
        wellen::FileFormat::Ghw => Ok(WaveFormat::Ghw),
        wellen::FileFormat::Unknown => bail!("Cannot parse {source}! Unknown format."),
    }
}

impl State {
    pub fn load_wave_from_file(
        &mut self,
        filename: Utf8PathBuf,
        load_options: LoadOptions,
    ) -> Result<()> {
        info!("Loading a waveform file: {filename}");
        let source = WaveSource::File(filename.clone());
        let sender = self.sys.channels.msg_sender.clone();

        perform_work(move || {
            let detected_format = wellen::open_and_detect_file_format(filename.as_str());
            let format = match check_format(&load_options, detected_format, &source) {
                Ok(format) => format,
                Err(e) => {
                    sender.send(Message::Error(e)).unwrap();
                    return;
                }
            };

            let result = match format {
                WaveFormat::Vcd => {
                    wellen::vcd::read_with_options(filename.as_str(), WELLEN_SURFER_DEFAULT_OPTIONS)
                        .map_err(|e| anyhow!("{e:?}"))
                        .with_context(|| format!("Failed to parse VCD file: {source}"))
                }
                WaveFormat::Fst => wellen::fst::read(filename.as_str())
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| format!("Failed to parse FST file: {source}")),
                WaveFormat::Ghw => wellen::ghw::read(filename.as_str())
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| format!("Failed to parse GHW file: {source}")),
            };

            match result {
                Ok(waves) => sender
                    .send(Message::WavesLoaded(
                        source,
                        format,
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

        let path = file.path.and_then(|x| Utf8PathBuf::try_from(x).ok());

        if let Some(bytes) = file.bytes {
            if bytes.len() == 0 {
                Err(anyhow!("Dropped an empty file"))
            } else {
                let total_bytes = bytes.len();
                self.load_vcd_from_bytes(
                    WaveSource::DragAndDrop(path),
                    bytes.to_vec(),
                    Some(total_bytes as u64),
                    LoadOptions::clean(),
                );
                Ok(())
            }
        } else if let Some(path) = path {
            self.load_wave_from_file(path, LoadOptions::clean())
        } else {
            Err(anyhow!(
                "Unknown how to load dropped file w/o path or bytes"
            ))
        }
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
            let detected_format = wellen::detect_file_format(&mut std::io::Cursor::new(&bytes));
            let format = match check_format(&load_options, detected_format, &source) {
                Ok(format) => format,
                Err(e) => {
                    sender.send(Message::Error(e)).unwrap();
                    return;
                }
            };

            let result = match format {
                WaveFormat::Vcd => {
                    wellen::vcd::read_from_bytes_with_options(&bytes, WELLEN_SURFER_DEFAULT_OPTIONS)
                        .map_err(|e| anyhow!("{e:?}"))
                        .with_context(|| format!("Failed to parse VCD file: {source}"))
                }
                WaveFormat::Fst => wellen::fst::read_from_bytes(bytes)
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| format!("Failed to parse FST file: {source}")),
                WaveFormat::Ghw => wellen::ghw::read_from_bytes(bytes)
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| format!("Failed to parse GHW file: {source}")),
            };

            match result {
                Ok(waves) => sender
                    .send(Message::WavesLoaded(
                        source,
                        format,
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
                .add_filter("Waveform-files (*.vcd, *.fst)", &["vcd", "fst"])
                .add_filter("All files", &["*"])
                .pick_file()
                .await
            {
                let keep_variables = match mode {
                    OpenMode::Open => false,
                    OpenMode::Switch => true,
                };

                #[cfg(not(target_arch = "wasm32"))]
                sender
                    .send(Message::LoadWaveformFile(
                        camino::Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap(),
                        LoadOptions {
                            keep_variables,
                            keep_unavailable,
                            expect_format: None,
                        },
                    ))
                    .unwrap();

                #[cfg(target_arch = "wasm32")]
                {
                    let data = file.read().await;
                    sender
                        .send(Message::LoadWaveformFileFromData(
                            data,
                            LoadOptions {
                                keep_variables,
                                keep_unavailable,
                                expect_format: None,
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
