use std::fmt::{Display, Formatter};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use crate::cxxrtl_container::CxxrtlContainer;
use crate::wasm_util::{perform_async_work, perform_work};
use camino::Utf8PathBuf;
use color_eyre::eyre::{anyhow, WrapErr};
use color_eyre::Result;
use eframe::egui::{self, DroppedFile};
use futures_util::FutureExt;
use futures_util::TryFutureExt;
use log::{error, info, warn};
use rfd::AsyncFileDialog;
use serde::{Deserialize, Serialize};

use crate::wellen::{LoadSignalsCmd, LoadSignalsResult};
use crate::{message::Message, wave_container::WaveContainer, State};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum WaveSource {
    File(Utf8PathBuf),
    Data,
    DragAndDrop(Option<Utf8PathBuf>),
    Url(String),
    #[cfg(not(target_arch = "wasm32"))]
    CxxrtlTcp(String),
}

pub fn url_to_wavesource(url: &str) -> Option<WaveSource> {
    if url.starts_with("https://") || url.starts_with("http://") {
        info!("Wave source is url");
        Some(WaveSource::Url(url.to_string()))
    } else if url.starts_with("cxxrtl+tcp://") {
        #[cfg(not(target_arch = "wasm32"))]
        {
            info!("Wave source is cxxrtl");
            Some(WaveSource::CxxrtlTcp(url.replace("cxxrtl+tcp://", "")))
        }
        #[cfg(target_arch = "wasm32")]
        {
            log::warn!("Loading waves from cxxrtl is unsupported in WASM builds.");
            None
        }
    } else {
        None
    }
}

pub fn string_to_wavesource(path: &str) -> WaveSource {
    if let Some(source) = url_to_wavesource(path) {
        source
    } else {
        info!("Wave source is file");
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
            #[cfg(not(target_arch = "wasm32"))]
            WaveSource::CxxrtlTcp(url) => write!(f, "{url}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum WaveFormat {
    Vcd,
    Fst,
    Ghw,
    CxxRtl,
}

impl Display for WaveFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            WaveFormat::Vcd => write!(f, "VCD"),
            WaveFormat::Fst => write!(f, "FST"),
            WaveFormat::Ghw => write!(f, "GHW"),
            WaveFormat::CxxRtl => write!(f, "Cxxrtl"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LoadOptions {
    pub keep_variables: bool,
    pub keep_unavailable: bool,
}

impl LoadOptions {
    pub fn clean() -> Self {
        Self {
            keep_variables: false,
            keep_unavailable: false,
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
    ReadingHeader(WaveSource),
    ReadingBody(WaveSource, u64, Arc<AtomicU64>),
    LoadingSignals(u64),
}

const WELLEN_SURFER_DEFAULT_OPTIONS: wellen::LoadOptions = wellen::LoadOptions {
    multi_thread: true,
    remove_scopes_with_empty_name: true,
};

impl State {
    pub fn load_wave_from_file(
        &mut self,
        filename: Utf8PathBuf,
        load_options: LoadOptions,
    ) -> Result<()> {
        info!("Loading a waveform file: {filename}");
        let start = web_time::Instant::now();
        let source = WaveSource::File(filename.clone());
        let source_copy = source.clone();
        let sender = self.sys.channels.msg_sender.clone();

        perform_work(move || {
            let header_result =
                wellen::viewers::read_header(filename.as_str(), &WELLEN_SURFER_DEFAULT_OPTIONS)
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| format!("Failed to parse wave file: {source}"));

            match header_result {
                Ok(header) => {
                    let msg = Message::WaveHeaderLoaded(start, source, load_options, header);
                    sender.send(msg).unwrap()
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });

        self.sys.progress_tracker = Some(LoadProgress::ReadingHeader(source_copy));
        Ok(())
    }

    pub fn load_wave_from_data(
        &mut self,
        vcd_data: Vec<u8>,
        load_options: LoadOptions,
    ) -> Result<()> {
        self.load_wave_from_bytes(WaveSource::Data, vcd_data, load_options);
        Ok(())
    }

    pub fn load_wave_from_dropped(&mut self, file: DroppedFile) -> Result<()> {
        info!("Got a dropped file");

        let path = file.path.and_then(|x| Utf8PathBuf::try_from(x).ok());

        if let Some(bytes) = file.bytes {
            if bytes.len() == 0 {
                Err(anyhow!("Dropped an empty file"))
            } else {
                self.load_wave_from_bytes(
                    WaveSource::DragAndDrop(path),
                    bytes.to_vec(),
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

    pub fn load_wave_from_url(&mut self, url: String, load_options: LoadOptions) {
        match url_to_wavesource(&url) {
            // We want to support opening cxxrtl urls using open url and friends,
            // so we'll special case
            #[cfg(not(target_arch = "wasm32"))]
            Some(WaveSource::CxxrtlTcp(url)) => {
                self.connect_to_cxxrtl(url, load_options.keep_variables)
            }
            // However, if we don't get a cxxrtl url, we want to continue loading this as
            // a url even if it isn't auto detected as a url.
            _ => {
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

                self.sys.progress_tracker = Some(LoadProgress::Downloading(url_))
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn connect_to_cxxrtl(&mut self, url: String, keep_variables: bool) {
        let sender = self.sys.channels.msg_sender.clone();
        let url_ = url.clone();
        let msg_sender = self.sys.channels.msg_sender.clone();
        let task = async move {
            let container = CxxrtlContainer::new(&url, msg_sender);

            match container {
                Ok(c) => sender.send(Message::WavesLoaded(
                    WaveSource::CxxrtlTcp(url),
                    WaveFormat::CxxRtl,
                    Box::new(WaveContainer::Cxxrtl(Mutex::new(c))),
                    LoadOptions {
                        keep_variables,
                        keep_unavailable: false,
                    },
                )),
                Err(e) => sender.send(Message::Error(e)),
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);

        self.sys.progress_tracker = Some(LoadProgress::Downloading(url_))
    }

    pub fn load_wave_from_bytes(
        &mut self,
        source: WaveSource,
        bytes: Vec<u8>,
        load_options: LoadOptions,
    ) {
        let start = web_time::Instant::now();
        let sender = self.sys.channels.msg_sender.clone();
        let source_copy = source.clone();
        perform_work(move || {
            let header_result =
                wellen::viewers::read_header_from_bytes(bytes, &WELLEN_SURFER_DEFAULT_OPTIONS)
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| format!("Failed to parse wave file: {source}"));

            match header_result {
                Ok(header) => {
                    let msg = Message::WaveHeaderLoaded(start, source, load_options, header);
                    sender.send(msg).unwrap()
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });

        self.sys.progress_tracker = Some(LoadProgress::ReadingHeader(source_copy));
    }

    fn get_thread_pool() -> Option<rayon::ThreadPool> {
        // try to create a new rayon thread pool so that we do not block drawing functionality
        // which might be blocked by the waveform reader using up all the threads in the global pool
        let pool = match rayon::ThreadPoolBuilder::new().build() {
            Ok(pool) => Some(pool),
            Err(e) => {
                // on wasm this will always fail
                warn!("failed to create thread pool: {e:?}");
                None
            }
        };
        pool
    }

    pub fn load_wave_body(
        &mut self,
        source: WaveSource,
        cont: wellen::viewers::ReadBodyContinuation,
        body_len: u64,
        hierarchy: Arc<wellen::Hierarchy>,
    ) {
        let start = web_time::Instant::now();
        let sender = self.sys.channels.msg_sender.clone();
        let source_copy = source.clone();
        let progress = Arc::new(AtomicU64::new(0));
        let progress_copy = progress.clone();
        let pool = Self::get_thread_pool();

        perform_work(move || {
            let action = || {
                let p = Some(progress_copy);
                let body_result = wellen::viewers::read_body(cont, &hierarchy, p)
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| format!("Failed to parse body of wave file: {source}"));

                match body_result {
                    Ok(body) => {
                        let msg = Message::WaveBodyLoaded(start, source, body);
                        sender.send(msg).unwrap()
                    }
                    Err(e) => sender.send(Message::Error(e)).unwrap(),
                }
            };
            if let Some(pool) = pool {
                pool.install(action);
            } else {
                action();
            }
        });

        self.sys.progress_tracker =
            Some(LoadProgress::ReadingBody(source_copy, body_len, progress));
    }

    pub fn load_signals(&mut self, cmd: LoadSignalsCmd) {
        let (mut source, signals, hierarchy, from_unique_id) = cmd.destruct();
        if signals.is_empty() {
            return;
        }
        let num_signals = signals.len() as u64;
        let start = web_time::Instant::now();
        let sender = self.sys.channels.msg_sender.clone();
        let pool = Self::get_thread_pool();

        perform_work(move || {
            let action = || {
                let loaded = source.load_signals(&signals, &hierarchy, true);
                let res = LoadSignalsResult::new(source, loaded, from_unique_id);
                let msg = Message::SignalsLoaded(start, res);
                sender.send(msg).unwrap();
            };
            if let Some(pool) = pool {
                pool.install(action);
            } else {
                action();
            }
        });

        self.sys.progress_tracker = Some(LoadProgress::LoadingSignals(num_signals));
    }

    pub fn open_file_dialog(&mut self, mode: OpenMode) {
        let sender = self.sys.channels.msg_sender.clone();
        let keep_unavailable = self.config.behavior.keep_during_reload;

        perform_async_work(async move {
            if let Some(file) = AsyncFileDialog::new()
                .set_title("Open waveform file")
                .add_filter(
                    "Waveform-files (*.vcd, *.fst, *.ghw)",
                    &["vcd", "fst", "ghw"],
                )
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
            LoadProgress::ReadingHeader(source) => {
                ui.spinner();
                ui.monospace(format!("Loading signal names from {source}"));
            }
            LoadProgress::ReadingBody(source, 0, _) => {
                ui.spinner();
                ui.monospace(format!("Loading signal change data from {source}"));
            }
            LoadProgress::LoadingSignals(num) => {
                ui.spinner();
                ui.monospace(format!("Loading {num} signals"));
            }
            LoadProgress::ReadingBody(source, total, bytes_done) => {
                let num_bytes = bytes_done.load(std::sync::atomic::Ordering::SeqCst);
                let progress = num_bytes as f32 / *total as f32;
                ui.monospace(format!(
                    "Loading signal change data from {source}. {} / {}",
                    bytesize::ByteSize::b(num_bytes),
                    bytesize::ByteSize::b(*total),
                ));
                let progress_bar = egui::ProgressBar::new(progress)
                    .show_percentage()
                    .desired_width(300.);
                ui.add(progress_bar);
            }
        });
    });
}
