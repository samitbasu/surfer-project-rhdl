use std::{
    collections::VecDeque,
    fs::File,
    io::Read,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use crate::{
    cxxrtl_container::CxxrtlContainer,
    wasm_util::{perform_async_work, perform_work},
};
use camino::Utf8PathBuf;
use color_eyre::eyre::{anyhow, WrapErr};
use color_eyre::Result;
use eframe::egui::{self, DroppedFile};
use fastwave_backend::parse_vcd;
use futures_util::FutureExt;
use futures_util::TryFutureExt;
use log::{error, info};
use progress_streams::ProgressReader;
use rfd::AsyncFileDialog;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

use crate::{message::Message, wave_container::WaveContainer, State};

#[derive(Debug, Serialize, Deserialize)]
pub enum WaveSource {
    File(Utf8PathBuf),
    Data,
    DragAndDrop(Option<Utf8PathBuf>),
    Url(String),
    CxxrtlTcp(String),
}

pub fn string_to_wavesource(path: String) -> WaveSource {
    if path.starts_with("https://") || path.starts_with("http://") {
        info!("Wave source is url");
        WaveSource::Url(path)
    } else if path.starts_with("cxxrtl+tcp://") {
        info!("Wave source is cxxrtl");
        WaveSource::CxxrtlTcp(path.replace("cxxrtl+tcp://", ""))
    } else {
        info!("Wave source is file");
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
            WaveSource::CxxrtlTcp(url) => write!(f, "{url}"),
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

impl State {
    pub fn load_vcd_from_file(
        &mut self,
        vcd_filename: Utf8PathBuf,
        keep_signals: bool,
    ) -> Result<()> {
        // We'll open the file to check if it exists here to panic the main thread if not.
        // Then we pass the file into the thread for parsing
        info!("Load VCD: {vcd_filename}");
        let file =
            File::open(&vcd_filename).with_context(|| format!("Failed to open {vcd_filename}"))?;
        let total_bytes = file
            .metadata()
            .map(|m| m.len())
            .map_err(|e| info!("Failed to get vcd file metadata {e}"))
            .ok();

        self.load_vcd(
            WaveSource::File(vcd_filename),
            file,
            total_bytes,
            keep_signals,
        );

        Ok(())
    }

    pub fn load_vcd_from_data(&mut self, vcd_data: Vec<u8>, keep_signals: bool) -> Result<()> {
        let total_bytes = vcd_data.len();

        self.load_vcd(
            WaveSource::Data,
            VecDeque::from(vcd_data),
            Some(total_bytes as u64),
            keep_signals,
        );
        Ok(())
    }

    pub fn load_vcd_from_dropped(&mut self, file: DroppedFile, keep_signals: bool) -> Result<()> {
        info!("Got a dropped file");

        let filename = file.path.and_then(|p| Utf8PathBuf::try_from(p).ok());
        let bytes = file
            .bytes
            .ok_or_else(|| anyhow!("Dropped a file with no bytes"))?;

        let total_bytes = bytes.len();

        self.load_vcd(
            WaveSource::DragAndDrop(filename),
            VecDeque::from_iter(bytes.iter().cloned()),
            Some(total_bytes as u64),
            keep_signals,
        );
        Ok(())
    }

    pub fn load_vcd_from_url(&mut self, url: String, keep_signals: bool) {
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
                Ok(b) => sender.send(Message::FileDownloaded(url, b, keep_signals)),
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

    pub fn connect_to_cxxrtl(&mut self, url: String, keep_signals: bool) {
        let sender = self.sys.channels.msg_sender.clone();
        let url_ = url.clone();
        let task = async move {
            let container = CxxrtlContainer::new(&url);

            match container {
                Ok(c) => sender.send(Message::WavesLoaded(
                    WaveSource::CxxrtlTcp(url),
                    Box::new(WaveContainer::Cxxrtl(c)),
                    keep_signals,
                )),
                Err(e) => sender.send(Message::Error(e)),
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);

        self.sys.vcd_progress = Some(LoadProgress::Downloading(url_))
    }

    pub fn load_vcd(
        &mut self,
        source: WaveSource,
        reader: impl Read + Send + 'static,
        total_bytes: Option<u64>,
        keep_signals: bool,
    ) {
        // Progress tracking in bytes
        let progress_bytes = Arc::new(AtomicU64::new(0));
        let reader = {
            info!("Creating progress reader");
            let progress_bytes = progress_bytes.clone();
            ProgressReader::new(reader, move |progress: usize| {
                progress_bytes.fetch_add(progress as u64, Ordering::SeqCst);
            })
        };

        let sender = self.sys.channels.msg_sender.clone();

        perform_work(move || {
            let result = parse_vcd(reader)
                .map_err(|e| anyhow!("{e}"))
                .with_context(|| format!("Failed to parse VCD file: {source}"));

            match result {
                Ok(waves) => sender
                    .send(Message::WavesLoaded(
                        source,
                        Box::new(WaveContainer::new_vcd(waves)),
                        keep_signals,
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
                        keep_signals,
                    ))
                    .unwrap();

                #[cfg(target_arch = "wasm32")]
                {
                    let data = file.read().await;
                    sender
                        .send(Message::LoadVcdFromData(data, keep_signals))
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
                    ui.monospace(format!("Loading. {num_bytes}/{total} kb loaded"));
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
