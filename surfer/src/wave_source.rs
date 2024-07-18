use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::Sender;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

#[cfg(not(target_arch = "wasm32"))]
use crate::cxxrtl_container::CxxrtlContainer;
use crate::wasm_util::{perform_async_work, perform_work};
use camino::{Utf8Path, Utf8PathBuf};
use color_eyre::eyre::{anyhow, WrapErr};
use color_eyre::Result;
use futures_util::FutureExt;
use log::{error, info, warn};
use rfd::AsyncFileDialog;
use serde::{Deserialize, Serialize};
use web_time::Instant;

use crate::message::{AsyncJob, BodyResult, HeaderResult};
#[cfg(not(target_arch = "wasm32"))]
use crate::wave_container::WaveContainer;
use crate::wellen::{LoadSignalPayload, LoadSignalsCmd, LoadSignalsResult};
use crate::{message::Message, State};
use surver::{Status, HTTP_SERVER_KEY, HTTP_SERVER_VALUE_SURFER, WELLEN_SURFER_DEFAULT_OPTIONS};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum WaveSource {
    File(Utf8PathBuf),
    Data,
    DragAndDrop(Option<Utf8PathBuf>),
    Url(String),
    #[cfg(not(target_arch = "wasm32"))]
    CxxrtlTcp(String),
}

impl WaveSource {
    pub fn as_file(&self) -> Option<&Utf8Path> {
        match self {
            WaveSource::File(path) => Some(path.as_path()),
            _ => None,
        }
    }
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

pub struct LoadProgress {
    pub started: Instant,
    pub progress: LoadProgressStatus,
}

impl LoadProgress {
    pub fn new(progress: LoadProgressStatus) -> Self {
        LoadProgress {
            started: Instant::now(),
            progress,
        }
    }
}

pub enum LoadProgressStatus {
    Downloading(String),
    ReadingHeader(WaveSource),
    ReadingBody(WaveSource, u64, Arc<AtomicU64>),
    LoadingVariables(u64),
}

macro_rules! spawn {
    ($task:expr) => {
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn($task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local($task);
    };
}

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
                    let msg = Message::WaveHeaderLoaded(
                        start,
                        source,
                        load_options,
                        HeaderResult::Local(Box::new(header)),
                    );
                    sender.send(msg).unwrap()
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });

        self.sys.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::ReadingHeader(
            source_copy,
        )));
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

    pub fn load_wave_from_dropped(&mut self, file: egui::DroppedFile) -> Result<()> {
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
                    let maybe_response = reqwest::get(&url)
                        .map(|e| e.with_context(|| format!("Failed fetch download {url}")))
                        .await;
                    let response: reqwest::Response = match maybe_response {
                        Ok(r) => r,
                        Err(e) => {
                            sender.send(Message::Error(e)).unwrap();
                            return;
                        }
                    };

                    // check to see if the response came from a Surfer running in server mode
                    if let Some(value) = response.headers().get(HTTP_SERVER_KEY) {
                        if matches!(value.to_str(), Ok(HTTP_SERVER_VALUE_SURFER)) {
                            info!("Connecting to a surfer server at: {url}");
                            // request status and hierarchy
                            Self::get_server_status(sender.clone(), url.clone(), 0);
                            Self::get_hierarchy_from_server(
                                sender.clone(),
                                url.clone(),
                                load_options,
                            );
                            return;
                        }
                    }

                    // otherwise we load the body to get at the file
                    let bytes = response
                        .bytes()
                        .map(|e| e.with_context(|| format!("Failed to download {url}")))
                        .await;

                    match bytes {
                        Ok(b) => sender.send(Message::FileDownloaded(url, b, load_options)),
                        Err(e) => sender.send(Message::Error(e)),
                    }
                    .unwrap();
                };
                spawn!(task);

                self.sys.progress_tracker =
                    Some(LoadProgress::new(LoadProgressStatus::Downloading(url_)))
            }
        }
    }
    fn get_hierarchy_from_server(
        sender: Sender<Message>,
        server: String,
        load_options: LoadOptions,
    ) {
        let start = web_time::Instant::now();
        let source = WaveSource::Url(server.clone());

        let task = async move {
            let res = crate::remote::get_hierarchy(server.clone())
                .await
                .map_err(|e| anyhow!("{e:?}"))
                .with_context(|| {
                    format!("Failed to retrieve hierarchy from remote server {server}")
                });

            match res {
                Ok(h) => {
                    let header = HeaderResult::Remote(Arc::new(h.hierarchy), h.file_format, server);
                    let msg = Message::WaveHeaderLoaded(start, source, load_options, header);
                    sender.send(msg).unwrap()
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        };
        spawn!(task);
    }

    pub fn get_time_table_from_server(sender: Sender<Message>, server: String) {
        let start = web_time::Instant::now();
        let source = WaveSource::Url(server.clone());

        let task = async move {
            let res = crate::remote::get_time_table(server.clone())
                .await
                .map_err(|e| anyhow!("{e:?}"))
                .with_context(|| {
                    format!("Failed to retrieve time table from remote server {server}")
                });

            match res {
                Ok(table) => {
                    let msg =
                        Message::WaveBodyLoaded(start, source, BodyResult::Remote(table, server));
                    sender.send(msg).unwrap()
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        };
        spawn!(task);
    }

    fn get_server_status(sender: Sender<Message>, server: String, delay_ms: u64) {
        let start = web_time::Instant::now();
        let task = async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            let res = crate::remote::get_status(server.clone())
                .await
                .map_err(|e| anyhow!("{e:?}"))
                .with_context(|| format!("Failed to retrieve status from remote server {server}"));

            match res {
                Ok(status) => {
                    let msg = Message::SurferServerStatus(start, server, status);
                    sender.send(msg).unwrap()
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        };
        spawn!(task);
    }

    /// uses the server status in order to display a loading bar
    pub fn server_status_to_progress(&mut self, server: String, status: Status) {
        // once the body is loaded, we are no longer interested in the status
        let body_loaded = self
            .waves
            .as_ref()
            .map(|w| w.inner.to_waves().unwrap().body_loaded())
            .unwrap_or(false);
        if !body_loaded {
            // the progress tracker will be cleared once the hierarchy is returned from the server
            let source = WaveSource::Url(server.clone());
            let sender = self.sys.channels.msg_sender.clone();
            self.sys.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::ReadingBody(
                source,
                status.bytes,
                Arc::new(AtomicU64::new(status.bytes_loaded)),
            )));
            // get another status update
            Self::get_server_status(sender, server, 250);
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
        spawn!(task);

        self.sys.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::Downloading(url_)))
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
                    let msg = Message::WaveHeaderLoaded(
                        start,
                        source,
                        load_options,
                        HeaderResult::Local(Box::new(header)),
                    );
                    sender.send(msg).unwrap()
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });

        self.sys.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::ReadingHeader(
            source_copy,
        )));
    }

    fn get_thread_pool() -> Option<rayon::ThreadPool> {
        // try to create a new rayon thread pool so that we do not block drawing functionality
        // which might be blocked by the waveform reader using up all the threads in the global pool
        match rayon::ThreadPoolBuilder::new().build() {
            Ok(pool) => Some(pool),
            Err(e) => {
                // on wasm this will always fail
                warn!("failed to create thread pool: {e:?}");
                None
            }
        }
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
                        let msg = Message::WaveBodyLoaded(start, source, BodyResult::Local(body));
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

        self.sys.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::ReadingBody(
            source_copy,
            body_len,
            progress,
        )));
    }

    pub fn load_variables(&mut self, cmd: LoadSignalsCmd) {
        let (signals, from_unique_id, payload) = cmd.destruct();
        if signals.is_empty() {
            return;
        }
        let num_signals = signals.len() as u64;
        let start = web_time::Instant::now();
        let sender = self.sys.channels.msg_sender.clone();

        match payload {
            LoadSignalPayload::Local(mut source, hierarchy) => {
                let pool = Self::get_thread_pool();

                perform_work(move || {
                    let action = || {
                        let loaded = source.load_signals(&signals, &hierarchy, true);
                        let res = LoadSignalsResult::local(source, loaded, from_unique_id);
                        let msg = Message::SignalsLoaded(start, res);
                        sender.send(msg).unwrap();
                    };
                    if let Some(pool) = pool {
                        pool.install(action);
                    } else {
                        action();
                    }
                });
            }
            LoadSignalPayload::Remote(server) => {
                let task = async move {
                    let res = crate::remote::get_signals(server.clone(), &signals)
                        .await
                        .map_err(|e| anyhow!("{e:?}"))
                        .with_context(|| {
                            format!("Failed to retrieve signals from remote server {server}")
                        });

                    match res {
                        Ok(loaded) => {
                            let res = LoadSignalsResult::remote(server, loaded, from_unique_id);
                            let msg = Message::SignalsLoaded(start, res);
                            sender.send(msg).unwrap()
                        }
                        Err(e) => sender.send(Message::Error(e)).unwrap(),
                    }
                };
                spawn!(task);
            }
        }

        self.sys.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::LoadingVariables(
            num_signals,
        )));
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
                        Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap(),
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

    pub fn load_state_file(&mut self, path: Option<PathBuf>) {
        let sender = self.sys.channels.msg_sender.clone();

        perform_async_work(async move {
            let source = if let Some(_path) = path.clone() {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    Some(_path.into())
                }
                #[cfg(target_arch = "wasm32")]
                {
                    None
                }
            } else {
                AsyncFileDialog::new()
                    .set_title("Load state")
                    .add_filter("Surfer state files (*.ron)", &["ron"])
                    .add_filter("All files", &["*"])
                    .pick_file()
                    .await
            };
            let Some(source) = source else {
                return;
            };
            let bytes = source.read().await;
            let new_state = match ron::de::from_bytes(&bytes)
                .context(format!("Failed loading {}", source.file_name()))
            {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to load state: {e:#?}");
                    return;
                }
            };
            sender.send(Message::LoadState(new_state, path)).unwrap();
        });
    }

    pub fn save_state_file(&mut self, path: Option<PathBuf>) {
        let sender = self.sys.channels.msg_sender.clone();
        let Some(encoded) = self.encode_state() else {
            return;
        };

        perform_async_work(async move {
            let destination = if let Some(_path) = path {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    Some(_path.into())
                }
                #[cfg(target_arch = "wasm32")]
                {
                    None
                }
            } else {
                AsyncFileDialog::new()
                    .set_title("Save state")
                    .add_filter("Surfer state files (*.ron)", &["ron"])
                    .add_filter("All files", &["*"])
                    .save_file()
                    .await
            };
            let Some(destination) = destination else {
                return;
            };

            #[cfg(not(target_arch = "wasm32"))]
            sender
                .send(Message::SetStateFile(destination.path().into()))
                .unwrap();
            destination
                .write(encoded.as_bytes())
                .await
                .map_err(|e| error!("Failed to write state to {destination:#?} {e:#?}"))
                .ok();
            sender
                .send(Message::AsyncDone(AsyncJob::SaveState))
                .unwrap();
        });
    }
}

pub fn draw_progress_information(ui: &mut egui::Ui, progress_data: &LoadProgress) {
    match &progress_data.progress {
        LoadProgressStatus::Downloading(url) => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.monospace(format!("Downloading {url}"));
            });
        }
        LoadProgressStatus::ReadingHeader(source) => {
            ui.spinner();
            ui.monospace(format!("Loading variable names from {source}"));
        }
        LoadProgressStatus::ReadingBody(source, 0, _) => {
            ui.spinner();
            ui.monospace(format!("Loading variable change data from {source}"));
        }
        LoadProgressStatus::LoadingVariables(num) => {
            ui.spinner();
            ui.monospace(format!("Loading {num} variables"));
        }
        LoadProgressStatus::ReadingBody(source, total, bytes_done) => {
            let num_bytes = bytes_done.load(std::sync::atomic::Ordering::SeqCst);
            let progress = num_bytes as f32 / *total as f32;
            ui.monospace(format!(
                "Loading variable change data from {source}. {} / {}",
                bytesize::ByteSize::b(num_bytes),
                bytesize::ByteSize::b(*total),
            ));
            let progress_bar = egui::ProgressBar::new(progress)
                .show_percentage()
                .desired_width(300.);
            ui.add(progress_bar);
        }
    };
}
