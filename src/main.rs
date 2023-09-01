mod benchmark;
mod command_prompt;
mod commands;
mod descriptors;
mod signal_canvas;
mod translation;
mod util;
mod view;
mod viewport;
mod wasm_util;

use camino::Utf8PathBuf;
use clap::Parser;
use color_eyre::eyre::anyhow;
use color_eyre::eyre::Context;
use color_eyre::Result;
use descriptors::ScopeDescriptor;
use descriptors::SignalDescriptor;
use eframe::egui;
use eframe::egui::DroppedFile;
use eframe::epaint::Vec2;
use fastwave_backend::parse_vcd;
use fastwave_backend::ScopeIdx;
use fastwave_backend::SignalIdx;
use fastwave_backend::VCD;
use fern::colors::ColoredLevelConfig;
use log::error;
use log::info;
use num::bigint::ToBigInt;
use num::BigInt;
use num::BigRational;
use num::FromPrimitive;
use num::ToPrimitive;
use progress_streams::ProgressReader;

use translation::spade::SpadeTranslator;
use translation::SignalInfo;
use translation::TranslationPreference;
use translation::Translator;
use translation::TranslatorList;
use view::TraceIdx;
use viewport::Viewport;
use wasm_util::perform_work;

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

#[derive(clap::Parser, Default)]
struct Args {
    vcd_file: Option<Utf8PathBuf>,
    #[clap(long)]
    spade_state: Option<Utf8PathBuf>,
    #[clap(long)]
    spade_top: Option<String>,
}

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    color_eyre::install()?;

    let colors = ColoredLevelConfig::new()
        .error(fern::colors::Color::Red)
        .warn(fern::colors::Color::Yellow)
        .info(fern::colors::Color::Green)
        .debug(fern::colors::Color::Blue)
        .trace(fern::colors::Color::White);

    let stdout_config = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}] {}",
                colors.color(record.level()),
                message
            ))
        })
        .chain(std::io::stdout());

    fern::Dispatch::new().chain(stdout_config).apply()?;

    let args = Args::parse();
    let state = State::new(args)?;

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1920., 1080.)),
        ..Default::default()
    };

    eframe::run_native("Surfer", options, Box::new(|_cc| Box::new(state)));

    Ok(())
}

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() -> Result<()> {
    color_eyre::install()?;

    // Make sure panics are logged using `console.error`.
    console_error_panic_hook::set_once();

    // Redirect tracing to console.log and friends:
    tracing_wasm::set_as_global_default();

    let web_options = eframe::WebOptions::default();

    let args = Args::default();
    let state = State::new(args)?;

    wasm_bindgen_futures::spawn_local(async {
        eframe::start_web(
            "the_canvas_id", // hardcode it
            web_options,
            Box::new(|cc| Box::new(state)),
        )
        .await
        .expect("failed to start eframe");
    });

    Ok(())
}

pub struct VcdData {
    inner: VCD,
    filename: String,
    active_scope: Option<ScopeIdx>,
    /// Root signals to display
    signals: Vec<(SignalIdx, SignalInfo)>,
    /// These hashmaps contain a list of all full (i.e., top.dut.mod1.signal) signal or scope
    /// names to their indices. They have to be initialized using the initialize_signal_scope_maps
    /// function after this struct is created.
    signals_to_ids: HashMap<String, SignalIdx>,
    scopes_to_ids: HashMap<String, ScopeIdx>,
    viewport: Viewport,
    num_timestamps: BigInt,
    /// Name of the translator used to translate this trace
    signal_format: HashMap<TraceIdx, String>,
    cursor: Option<BigInt>,
    focused_signal: Option<usize>,
}

pub enum MoveDir {
    Up,
    Down,
}

pub enum Message {
    SetActiveScope(ScopeDescriptor),
    AddSignal(SignalDescriptor),
    AddScope(ScopeDescriptor),
    RemoveSignal(usize),
    FocusSignal(usize),
    UnfocusSignal,
    MoveFocus(MoveDir),
    MoveFocusedSignal(MoveDir),
    SignalFormatChange(TraceIdx, String),
    // Reset the translator for this signal back to default. Sub-signals,
    // i.e. those with the signal idx and a shared path are also reset
    ResetSignalFormat(TraceIdx),
    CanvasScroll {
        delta: Vec2,
    },
    CanvasZoom {
        mouse_ptr_timestamp: BigRational,
        delta: f32,
    },
    CursorSet(BigInt),
    LoadVcd(Utf8PathBuf),
    VcdLoaded(Utf8PathBuf, Box<VCD>),
    Error(color_eyre::eyre::Error),
    TranslatorLoaded(Box<dyn Translator + Send>),
    /// Take note that the specified translator errored on a `translates` call on the
    /// specified signal
    BlacklistTranslator(SignalIdx, String),
    ToggleSidePanel,
    ShowCommandPrompt(bool),
    FileDroped(DroppedFile),
}

pub struct State {
    vcd: Option<VcdData>,
    /// The offset of the left side of the wave window in signal timestamps.
    control_key: bool,
    /// Which translator to use for each signal
    translators: TranslatorList,

    /// Receiver for messages generated by other threads
    msg_sender: Sender<Message>,
    msg_receiver: Receiver<Message>,

    /// The number of bytes loaded from the vcd file
    vcd_progress: Option<(Option<u64>, Arc<AtomicU64>)>,

    // Vector of translators which have failed at the `translates` function for a signal.
    blacklisted_translators: HashSet<(SignalIdx, String)>,
    /// buffer for the command input
    command_prompt: command_prompt::CommandPrompt,
    /// Flag to show/hide the side panel
    show_side_panel: bool,
    // Flag to show the signal selectors i.e., aa, ab, ac, ...
    // show_signal_selectors: bool,
}

impl State {
    fn new(args: Args) -> Result<State> {
        let (sender, receiver) = channel();

        // Basic translators that we can load quickly
        let translators = TranslatorList::new(
            vec![
                Box::new(translation::HexTranslator {}),
                Box::new(translation::UnsignedTranslator {}),
                Box::new(translation::SignedTranslator {}),
                Box::new(translation::ExtendingBinaryTranslator {}),
            ],
            vec![],
        );

        // Long running translators which we load in a thread
        {
            let sender = sender.clone();
            perform_work(move || {
                if let (Some(top), Some(state)) = (args.spade_top, args.spade_state) {
                    let t = SpadeTranslator::new(&top, &state);
                    match t {
                        Ok(result) => sender
                            .send(Message::TranslatorLoaded(Box::new(result)))
                            .unwrap(),
                        Err(e) => sender.send(Message::Error(e)).unwrap(),
                    }
                } else {
                    info!("spade-top and spade-state not set, not loading spade translator");
                }
            });
        }

        let vcd_load_status = args
            .vcd_file
            .map(|path| State::load_vcd_from_file(path, sender.clone()).ok())
            .flatten();

        Ok(State {
            vcd: None,
            control_key: false,
            translators,
            msg_sender: sender,
            msg_receiver: receiver,
            vcd_progress: vcd_load_status,
            blacklisted_translators: HashSet::new(),
            command_prompt: command_prompt::CommandPrompt {
                visible: false,
                input: String::from(""),
                expanded: String::from(""),
                suggestions: vec![],
            },
            show_side_panel: true,
        })
    }

    fn load_vcd_from_file(
        vcd_filename: Utf8PathBuf,
        sender: Sender<Message>,
    ) -> Result<(Option<u64>, Arc<AtomicU64>)> {
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

        Self::load_vcd(Some(vcd_filename), file, total_bytes, sender)
    }

    fn load_vcd_from_dropped(
        file: DroppedFile,
        sender: Sender<Message>,
    ) -> Result<(Option<u64>, Arc<AtomicU64>)> {
        info!("Got a dropped file");

        let filename = file
            .path
            .map(|p| Utf8PathBuf::try_from(p).unwrap_or_default());
        let bytes = file
            .bytes
            .ok_or_else(|| anyhow!("Dropped a file with no bytes"))?;

        let total_bytes = bytes.len();

        // TODO: Copying the whole vcd file here is inefficient. We should try to make
        // a reader out of the Arc<[u8]>
        Self::load_vcd(
            filename,
            VecDeque::from_iter(bytes.into_iter().cloned()),
            Some(total_bytes as u64),
            sender,
        )
    }

    fn load_vcd(
        filename: Option<Utf8PathBuf>,
        reader: impl Read + Send + 'static,
        total_bytes: Option<u64>,
        sender: Sender<Message>,
    ) -> Result<(Option<u64>, Arc<AtomicU64>)> {
        // Progress tracking in bytes
        let progress_bytes = Arc::new(AtomicU64::new(0));
        let reader = {
            info!("Creating progress reader");
            let progress_bytes = progress_bytes.clone();
            ProgressReader::new(reader, move |progress: usize| {
                progress_bytes.fetch_add(progress as u64, Ordering::SeqCst);
            })
        };

        perform_work(move || {
            info!("Loading VCD");
            let result = parse_vcd(reader)
                .map_err(|e| anyhow!("{e}"))
                .with_context(|| format!("Failed to parse parse vcd file"));

            match result {
                Ok(vcd) => sender
                    .send(Message::VcdLoaded(
                        filename.unwrap_or_default(),
                        Box::new(vcd),
                    ))
                    .unwrap(),
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });

        Ok((total_bytes, progress_bytes))
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::SetActiveScope(descriptor) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                if let Some(scope) = descriptor.resolve(vcd) {
                    vcd.active_scope = Some(scope)
                }
            }
            Message::AddSignal(descriptor) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                if let Some(id) = descriptor.resolve(vcd) {
                    vcd.add_signal(&self.translators, id)
                }
            }
            Message::AddScope(descriptor) => {
                let Some(vcd) = self.vcd.as_mut() else { return };

                if let Some(s) = descriptor.resolve(vcd) {
                    let signals = vcd.inner.get_children_signal_idxs(s);
                    for sidx in signals {
                        if !vcd.signal_name(sidx).starts_with("_") {
                            vcd.add_signal(&self.translators, sidx);
                        }
                    }
                }
            }
            Message::FocusSignal(idx) => {
                let Some(vcd) = self.vcd.as_mut() else { return };

                let visible_signals_len = vcd.signals.len();
                if visible_signals_len > 0 && idx <= visible_signals_len - 1 {
                    vcd.focused_signal = Some(idx);
                } else {
                    error!(
                        "Can not focus signal {idx} because only {} signals are visible.",
                        vcd.signals.len()
                    );
                }
            }
            Message::UnfocusSignal => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                vcd.focused_signal = None;
            }
            Message::MoveFocus(direction) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                let visible_signals_len = vcd.signals.len();
                if visible_signals_len > 0 {
                    match direction {
                        MoveDir::Up => {
                            vcd.focused_signal = vcd.focused_signal.map_or(
                                Some(visible_signals_len - 1),
                                |focused| {
                                    if focused > 0 {
                                        Some(focused - 1)
                                    } else {
                                        Some(focused)
                                    }
                                },
                            )
                        }
                        MoveDir::Down => {
                            vcd.focused_signal = vcd.focused_signal.map_or(Some(0), |focused| {
                                if focused < (visible_signals_len - 1).try_into().unwrap_or(0) {
                                    Some(focused + 1)
                                } else {
                                    Some(focused)
                                }
                            });
                        }
                    }
                }
            }
            Message::RemoveSignal(idx) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                let visible_signals_len = vcd.signals.len();
                if visible_signals_len > 0 && idx <= (visible_signals_len - 1) {
                    vcd.signals.remove(idx);
                    if let Some(focused) = vcd.focused_signal {
                        if focused == idx {
                            if (idx > 0) && (idx == (visible_signals_len - 1)) {
                                // if the end of list is selected
                                vcd.focused_signal = Some(idx - 1);
                            }
                        } else {
                            if idx < focused {
                                vcd.focused_signal = Some(focused - 1)
                            }
                        }
                        if vcd.signals.is_empty() {
                            vcd.focused_signal = None;
                        }
                    }
                }
            }
            Message::MoveFocusedSignal(direction) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                if let Some(idx) = vcd.focused_signal {
                    let visible_signals_len = vcd.signals.len();
                    if visible_signals_len > 0 {
                        match direction {
                            MoveDir::Up => {
                                if idx > 0 {
                                    vcd.signals.swap(idx, idx - 1);
                                    vcd.focused_signal = Some(idx - 1);
                                }
                            }
                            MoveDir::Down => {
                                if idx < (visible_signals_len - 1) {
                                    vcd.signals.swap(idx, idx + 1);
                                    vcd.focused_signal = Some(idx + 1);
                                }
                            }
                        }
                    }
                }
            }
            Message::CanvasScroll { delta } => self.handle_canvas_scroll(delta),
            Message::CanvasZoom {
                delta,
                mouse_ptr_timestamp,
            } => {
                self.vcd
                    .as_mut()
                    .map(|vcd| vcd.handle_canvas_zoom(mouse_ptr_timestamp, delta as f64));
            }
            Message::SignalFormatChange(ref idx @ (ref signal_idx, ref path), format) => {
                let Some(vcd) = self.vcd.as_mut() else { return };

                if self.translators.all_translator_names().contains(&&format) {
                    *vcd.signal_format.entry(idx.clone()).or_default() = format;

                    if path.is_empty() {
                        let signal = vcd.inner.signal_from_signal_idx(idx.0);
                        let translator = vcd.signal_translator(idx.clone(), &self.translators);
                        let new_info = translator
                            .signal_info(&signal, &vcd.signal_name(idx.0))
                            .unwrap();

                        for (i, info) in &mut vcd.signals {
                            if i == signal_idx {
                                *info = new_info;
                                break;
                            }
                        }
                    }
                } else {
                    println!("WARN: No translator {format}")
                }
            }
            Message::ResetSignalFormat(idx) => {
                self.vcd.as_mut().map(|vcd| vcd.signal_format.remove(&idx));
            }
            Message::CursorSet(new) => {
                self.vcd.as_mut().map(|vcd| vcd.cursor = Some(new));
            }
            Message::LoadVcd(filename) => {
                self.vcd_progress =
                    State::load_vcd_from_file(filename, self.msg_sender.clone()).ok();
            }
            Message::FileDroped(dropped_file) => {
                self.vcd_progress =
                    Self::load_vcd_from_dropped(dropped_file, self.msg_sender.clone())
                        .map_err(|e| error!("{e:#?}"))
                        .ok();
            }
            Message::VcdLoaded(filename, new_vcd_data) => {
                info!("VCD file loaded");
                let num_timestamps = new_vcd_data
                    .max_timestamp()
                    .as_ref()
                    .map(|t| t.to_bigint().unwrap())
                    .unwrap_or(BigInt::from_u32(1).unwrap());

                let mut new_vcd = VcdData {
                    inner: *new_vcd_data,
                    filename: filename.to_string(),
                    active_scope: None,
                    signals: vec![],
                    signals_to_ids: HashMap::new(),
                    scopes_to_ids: HashMap::new(),
                    viewport: Viewport::new(0., num_timestamps.clone().to_f64().unwrap()),
                    signal_format: HashMap::new(),
                    num_timestamps,
                    cursor: None,
                    focused_signal: None,
                };
                new_vcd.initialize_signal_scope_maps();

                self.vcd = Some(new_vcd);
                info!("Done setting up vcd file");
            }
            Message::BlacklistTranslator(idx, translator) => {
                self.blacklisted_translators.insert((idx, translator));
            }
            Message::Error(e) => {
                error!("{e:?}")
            }
            Message::TranslatorLoaded(t) => {
                info!("Translator {} loaded", t.name());
                self.translators.add(t)
            }
            Message::ToggleSidePanel => {
                self.show_side_panel = !self.show_side_panel;
            }
            Message::ShowCommandPrompt(new_visibility) => {
                if !new_visibility {
                    self.command_prompt.input = "".to_string();
                    self.command_prompt.suggestions = vec![];
                    self.command_prompt.expanded = "".to_string();
                }
                self.command_prompt.visible = new_visibility;
            }
        }
    }

    pub fn handle_canvas_scroll(
        &mut self,
        // Canvas relative
        delta: Vec2,
    ) {
        if let Some(vcd) = &mut self.vcd {
            // Scroll 5% of the viewport per scroll event.
            // One scroll event yields 50
            let scroll_step = (&vcd.viewport.curr_right - &vcd.viewport.curr_left) / (50. * 20.);

            let target_left = &vcd.viewport.curr_left + scroll_step * delta.y as f64;
            let target_right = &vcd.viewport.curr_right + scroll_step * delta.y as f64;

            vcd.viewport.curr_left = target_left;
            vcd.viewport.curr_right = target_right;
        }
    }
}

impl VcdData {
    pub fn signal_name(&self, idx: SignalIdx) -> String {
        self.inner.signal_from_signal_idx(idx).name()
    }

    pub fn select_preferred_translator(
        &self,
        sig: SignalIdx,
        translators: &TranslatorList,
    ) -> String {
        translators
            .all_translators()
            .iter()
            .filter_map(|t| {
                let signal = self.inner.signal_from_signal_idx(sig);
                match t.translates(&signal) {
                    Ok(TranslationPreference::Prefer) => Some(t.name()),
                    Ok(TranslationPreference::Yes) => None,
                    Ok(TranslationPreference::No) => None,
                    Err(e) => {
                        error!(
                            "Failed to check if {} translates {}\n{e:#?}",
                            t.name(),
                            signal.name()
                        );
                        None
                    }
                }
            })
            .next()
            .unwrap_or(translators.default.clone())
    }

    pub fn signal_translator<'a>(
        &'a self,
        sig: TraceIdx,
        translators: &'a TranslatorList,
    ) -> &'a dyn Translator {
        let translator_name = self.signal_format.get(&sig).cloned().unwrap_or_else(|| {
            if sig.1.is_empty() {
                self.select_preferred_translator(sig.0, translators)
            } else {
                translators.default.clone()
            }
        });
        let translator = translators.get_translator(&translator_name);
        translator
    }

    pub fn handle_canvas_zoom(
        &mut self,
        // Canvas relative
        mouse_ptr_timestamp: BigRational,
        delta: f64,
    ) {
        // Zoom or scroll
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self.viewport;

        let target_left = (left - mouse_ptr_timestamp.to_f64().unwrap()) / delta
            + &mouse_ptr_timestamp.to_f64().unwrap();
        let target_right = (right - mouse_ptr_timestamp.to_f64().unwrap()) / delta
            + &mouse_ptr_timestamp.to_f64().unwrap();

        self.viewport.curr_left = target_left;
        self.viewport.curr_right = target_right;
    }

    // Initializes the scopes_to_ids and signals_to_ids
    // fields by iterating down the scope hierarchy and collectiong
    // the absolute names of all signals and scopes
    pub fn initialize_signal_scope_maps(&mut self) {
        // in scope S and path P, adds all signals x to all_signal_names
        // as [S.]P.x
        // does the same for scopes
        // goes down into subscopes and does the same there
        fn add_scope_signals(scope: ScopeIdx, path: String, vcd: &mut VcdData) {
            let scope_name = vcd.inner.scope_name_by_idx(scope);
            let full_scope_name = if !path.is_empty() {
                format!("{path}.{}", scope_name)
            } else {
                scope_name.to_string()
            };
            vcd.scopes_to_ids.insert(full_scope_name.clone(), scope);

            let signal_idxs = vcd.inner.get_children_signal_idxs(scope);
            for signal in signal_idxs {
                let signal_name = vcd.inner.signal_from_signal_idx(signal).name();
                if !signal_name.starts_with('_') {
                    vcd.signals_to_ids
                        .insert(format!("{}.{}", full_scope_name, signal_name), signal);
                }
            }

            for sub_scope in vcd.inner.child_scopes_by_idx(scope) {
                add_scope_signals(sub_scope, full_scope_name.clone(), vcd);
            }
        }

        for root_scope in self.inner.root_scopes_by_idx() {
            add_scope_signals(root_scope, String::from(""), self);
        }
    }

    pub fn add_signal(&mut self, translators: &TranslatorList, sidx: SignalIdx) {
        let signal = self.inner.signal_from_signal_idx(sidx);
        let translator = self.signal_translator((sidx, vec![]), translators);
        let info = translator
            .signal_info(&signal, &self.signal_name(sidx))
            .unwrap();
        self.signals.push((sidx, info))
    }
}
