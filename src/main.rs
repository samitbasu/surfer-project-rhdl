mod benchmark;
mod command_prompt;
mod commands;
mod config;
mod descriptors;
mod signal_canvas;
#[cfg(test)]
mod tests;
mod translation;
mod util;
mod view;
mod viewport;
mod wasm_util;

use bytes::Buf;
use bytes::Bytes;
use camino::Utf8PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
use color_eyre::eyre::anyhow;
use color_eyre::eyre::Context;
use color_eyre::Result;
use derivative::Derivative;
use descriptors::PathDescriptor;
use descriptors::ScopeDescriptor;
use descriptors::SignalDescriptor;
#[cfg(not(target_arch = "wasm32"))]
use eframe::egui;
use eframe::egui::style::Selection;
use eframe::egui::style::WidgetVisuals;
use eframe::egui::style::Widgets;
use eframe::egui::DroppedFile;
use eframe::egui::Visuals;
use eframe::emath;
use eframe::epaint::Pos2;
use eframe::epaint::Rect;
use eframe::epaint::Rounding;
use eframe::epaint::Stroke;
use eframe::epaint::Vec2;
use fastwave_backend::parse_vcd;
use fastwave_backend::ScopeIdx;
use fastwave_backend::SignalIdx;
use fastwave_backend::Timescale;
use fastwave_backend::VCD;
#[cfg(not(target_arch = "wasm32"))]
use fern::colors::ColoredLevelConfig;
use futures_util::FutureExt;
use futures_util::TryFutureExt;
use itertools::Itertools;
use log::error;
use log::info;
use log::trace;
use num::bigint::ToBigInt;
use num::BigInt;
use num::FromPrimitive;
use num::ToPrimitive;
use progress_streams::ProgressReader;
#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;
use serde::Deserialize;
use translation::spade::SpadeTranslator;
use translation::SignalInfo;
use translation::TranslationPreference;
use translation::Translator;
use translation::TranslatorList;
use view::TraceIdx;
use viewport::Viewport;
use wasm_util::perform_work;

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::str::FromStr;
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

struct StartupParams {
    pub spade_state: Option<Utf8PathBuf>,
    pub spade_top: Option<String>,
    pub vcd: Option<WaveSource>,
}

impl StartupParams {
    pub fn empty() -> Self {
        Self {
            spade_state: None,
            spade_top: None,
            vcd: None,
        }
    }

    #[allow(dead_code)] // NOTE: Only used in wasm version
    pub fn vcd_from_url(url: Option<String>) -> Self {
        Self {
            spade_state: None,
            spade_top: None,
            vcd: url.map(WaveSource::Url),
        }
    }

    #[allow(dead_code)] // NOTE: Only used in desktop version
    pub fn from_args(args: Args) -> Self {
        Self {
            spade_state: args.spade_state,
            spade_top: args.spade_top,
            vcd: args.vcd_file.map(WaveSource::File),
        }
    }
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

    // https://tokio.rs/tokio/topics/bridging
    // We want to run the gui in the main thread, but some long running tasks like
    // laoading VCDs should be done asynchronously. We can't just use std::thread to
    // do that due to wasm support, so we'll start a tokio runtime
    let runtime = tokio::runtime::Builder::new_current_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();

    let _enter = runtime.enter();

    std::thread::spawn(move || {
        runtime.block_on(async {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            }
        })
    });

    let args = Args::parse();
    let mut state = State::new(StartupParams::from_args(args))?;

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1920., 1080.)),
        ..Default::default()
    };

    eframe::run_native(
        "Surfer",
        options,
        Box::new(|cc| {
            state.context = Some(cc.egui_ctx.clone());
            cc.egui_ctx.set_visuals(state.get_visuals());
            Box::new(state)
        }),
    )
    .unwrap();

    Ok(())
}

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() -> Result<()> {
    console_error_panic_hook::set_once();
    color_eyre::install()?;
    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    let url = wasm_util::vcd_from_url();

    let state = State::new(StartupParams::vcd_from_url(url))?;

    wasm_bindgen_futures::spawn_local(async {
        eframe::WebRunner::new()
            .start(
                "the_canvas_id", // hardcode it
                web_options,
                Box::new(|cc| {
                    cc.egui_ctx.set_visuals(state.get_visuals());
                    Box::new(state)
                }),
            )
            .await
            .expect("failed to start eframe");
    });

    Ok(())
}

#[derive(Debug)]
pub enum WaveSource {
    File(Utf8PathBuf),
    DragAndDrop(Option<Utf8PathBuf>),
    Url(String),
}

impl std::fmt::Display for WaveSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaveSource::File(file) => write!(f, "{file}"),
            WaveSource::DragAndDrop(None) => write!(f, "Dropped file"),
            WaveSource::DragAndDrop(Some(filename)) => write!(f, "Dropped file ({filename})"),
            WaveSource::Url(url) => write!(f, "{url}"),
        }
    }
}

#[derive(PartialEq, Copy, Clone, Debug, Deserialize)]
pub enum SignalNameType {
    Local,  // local signal name only (i.e. for tb.dut.clk => clk)
    Unique, // add unique prefix, prefix + local
    Global, // full signal name (i.e. tb.dut.clk => tb.dut.clk)
}

impl FromStr for SignalNameType {
    type Err = String;

    fn from_str(input: &str) -> Result<SignalNameType, Self::Err> {
        match input {
            "Local" => Ok(SignalNameType::Local),
            "Unique" => Ok(SignalNameType::Unique),
            "Global" => Ok(SignalNameType::Global),
            _ => Err(format!(
                "'{}' is not a valid SignalNameType (Valid options: Local|Unique|Global)",
                input
            )),
        }
    }
}

impl std::fmt::Display for SignalNameType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalNameType::Local => write!(f, "Local"),
            SignalNameType::Unique => write!(f, "Unique"),
            SignalNameType::Global => write!(f, "Global"),
        }
    }
}

#[derive(PartialEq, Copy, Clone, Debug, Deserialize)]
pub enum ClockHighlightType {
    Line,  // Draw a line at every posedge of the clokcs
    Cycle, // Highlight every other cycle
    None,  // No highlighting
}

impl FromStr for ClockHighlightType {
    type Err = String;

    fn from_str(input: &str) -> Result<ClockHighlightType, Self::Err> {
        match input {
            "Line" => Ok(ClockHighlightType::Line),
            "Cycle" => Ok(ClockHighlightType::Cycle),
            "None" => Ok(ClockHighlightType::None),
            _ => Err(format!(
                "'{}' is not a valid ClockHighlightType (Valid options: Line|Cycle|None)",
                input
            )),
        }
    }
}

impl std::fmt::Display for ClockHighlightType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClockHighlightType::Line => write!(f, "Line"),
            ClockHighlightType::Cycle => write!(f, "Cycle"),
            ClockHighlightType::None => write!(f, "None"),
        }
    }
}

pub struct DisplayedSignal {
    idx: SignalIdx,
    info: SignalInfo,
    color: Option<String>,
    background_color: Option<String>,
    display_name: String,
    display_name_type: SignalNameType,
}

pub struct DisplayedDivider {
    color: Option<String>,
    background_color: Option<String>,
    name: String,
}

pub struct DisplayedCursor {
    color: Option<String>,
    background_color: Option<String>,
    name: String,
    idx: u8,
}

enum DisplayedItem {
    Signal(DisplayedSignal),
    Divider(DisplayedDivider),
    Cursor(DisplayedCursor),
}

impl DisplayedItem {
    pub fn color(&self) -> Option<String> {
        let color = match self {
            DisplayedItem::Signal(signal) => &signal.color,
            DisplayedItem::Divider(divider) => &divider.color,
            DisplayedItem::Cursor(cursor) => &cursor.color,
        };
        color.clone()
    }

    pub fn set_color(&mut self, color_name: Option<String>) {
        match self {
            DisplayedItem::Signal(signal) => {
                signal.color = color_name.clone();
            }
            DisplayedItem::Divider(divider) => {
                divider.color = color_name.clone();
            }
            DisplayedItem::Cursor(cursor) => {
                cursor.color = color_name.clone();
            }
        }
    }

    pub fn name(&self) -> String {
        let name = match self {
            DisplayedItem::Signal(signal) => &signal.display_name,
            DisplayedItem::Divider(divider) => &divider.name,
            DisplayedItem::Cursor(cursor) => &cursor.name,
        };
        name.clone()
    }

    pub fn set_name(&mut self, name: String) {
        match self {
            DisplayedItem::Signal(signal) => {
                signal.display_name = name.clone();
            }
            DisplayedItem::Divider(divider) => {
                divider.name = name.clone();
            }
            DisplayedItem::Cursor(cursor) => {
                cursor.name = name.clone();
            }
        }
    }

    pub fn background_color(&self) -> Option<String> {
        let background_color = match self {
            DisplayedItem::Signal(signal) => &signal.background_color,
            DisplayedItem::Divider(divider) => &divider.background_color,
            DisplayedItem::Cursor(cursor) => &cursor.background_color,
        };
        background_color.clone()
    }

    pub fn set_background_color(&mut self, color_name: Option<String>) {
        match self {
            DisplayedItem::Signal(signal) => {
                signal.background_color = color_name.clone();
            }
            DisplayedItem::Divider(divider) => {
                divider.background_color = color_name.clone();
            }
            DisplayedItem::Cursor(cursor) => {
                cursor.background_color = color_name.clone();
            }
        }
    }
}

pub struct VcdData {
    inner: VCD,
    filename: String,
    active_scope: Option<ScopeIdx>,
    /// Root items (signals, dividers, ...) to display
    displayed_items: Vec<DisplayedItem>,
    /// These hashmaps contain a list of all full (i.e., top.dut.mod1.signal) signal or scope
    /// names to their indices. They have to be initialized using the initialize_signal_scope_maps
    /// function after this struct is created.
    signals_to_ids: HashMap<String, SignalIdx>,
    scopes_to_ids: HashMap<String, ScopeIdx>,
    /// Maps signal indices to the corresponding full signal name (i.e. top.sub.signal)
    ids_to_fullnames: HashMap<SignalIdx, String>,
    viewport: Viewport,
    num_timestamps: BigInt,
    /// Name of the translator used to translate this trace
    signal_format: HashMap<TraceIdx, String>,
    cursor: Option<BigInt>,
    cursors: HashMap<u8, BigInt>,
    focused_item: Option<usize>,
    default_signal_name_type: SignalNameType,
    scroll: usize,
}

type CommandCount = usize;

#[derive(Debug)]
pub enum MoveDir {
    Up,
    Down,
}

pub enum ColorSpecifier {
    Index(usize),
    Name(String),
}

#[derive(Derivative)]
#[derivative(Debug)]
pub enum Message {
    SetActiveScope(ScopeDescriptor),
    AddSignal(SignalDescriptor),
    AddScope(ScopeDescriptor),
    AddCount(char),
    InvalidateCount,
    RemoveItem(usize, CommandCount),
    FocusItem(usize),
    RenameItem(usize),
    UnfocusItem,
    MoveFocus(MoveDir, CommandCount),
    MoveFocusedItem(MoveDir, CommandCount),
    VerticalScroll(MoveDir, CommandCount),
    SetVerticalScroll(usize),
    SignalFormatChange(PathDescriptor, String),
    ItemColorChange(Option<usize>, Option<String>),
    ItemBackgroundColorChange(Option<usize>, Option<String>),
    ItemNameChange(Option<usize>, String),
    ChangeSignalNameType(Option<usize>, SignalNameType),
    ForceSignalNameTypes(SignalNameType),
    SetClockHighlightType(ClockHighlightType),
    // Reset the translator for this signal back to default. Sub-signals,
    // i.e. those with the signal idx and a shared path are also reset
    ResetSignalFormat(TraceIdx),
    CanvasScroll {
        delta: Vec2,
    },
    CanvasZoom {
        mouse_ptr_timestamp: Option<f64>,
        delta: f32,
    },
    ZoomToRange {
        start: f64,
        end: f64,
    },
    CursorSet(BigInt),
    LoadVcd(Utf8PathBuf),
    LoadVcdFromUrl(String),
    VcdLoaded(WaveSource, Box<VCD>),
    Error(color_eyre::eyre::Error),
    TranslatorLoaded(#[derivative(Debug = "ignore")] Box<dyn Translator + Send>),
    /// Take note that the specified translator errored on a `translates` call on the
    /// specified signal
    BlacklistTranslator(SignalIdx, String),
    ToggleSidePanel,
    ShowCommandPrompt(bool),
    FileDropped(DroppedFile),
    FileDownloaded(String, Bytes),
    ReloadConfig,
    ZoomToFit,
    GoToStart,
    GoToEnd,
    ToggleMenu,
    SetTimeScale(Timescale),
    CommandPromptClear,
    CommandPromptUpdate {
        expanded: String,
        suggestions: Vec<(String, Vec<bool>)>,
    },
    OpenFileDialog,
    SetAboutVisible(bool),
    SetKeyHelpVisible(bool),
    SetUrlEntryVisible(bool),
    SetRenameItemVisible(bool),
    SetDragStart(Option<Pos2>),
    SetFilterFocused(bool),
    ToggleFullscreen,
    AddDivider(String),
    SetCursorPosition(u8),
    GoToCursorPosition(u8),
    /// Exit the application. This has no effect on wasm and closes the window
    /// on other platforms
    Exit,
}

pub enum LoadProgress {
    Downloading(String),
    Loading(Option<u64>, Arc<AtomicU64>),
}

struct CachedDrawData {
    pub draw_commands: HashMap<(SignalIdx, Vec<String>), signal_canvas::DrawingCommands>,
    pub clock_edges: Vec<f32>,
}

pub struct State {
    config: config::SurferConfig,
    vcd: Option<VcdData>,
    /// Count argument for movements
    count: Option<String>,
    /// Which translator to use for each signal
    translators: TranslatorList,

    /// Receiver for messages generated by other threads
    msg_sender: Sender<Message>,
    msg_receiver: Receiver<Message>,

    /// The number of bytes loaded from the vcd file
    vcd_progress: Option<LoadProgress>,

    // Vector of translators which have failed at the `translates` function for a signal.
    blacklisted_translators: HashSet<(SignalIdx, String)>,
    /// Buffer for the command input
    command_prompt: command_prompt::CommandPrompt,

    /// The context to egui, we need this to change the visual settings when the config is reloaded
    context: Option<eframe::egui::Context>,

    show_about: bool,
    show_keys: bool,
    /// Hide the wave source. For now, this is only used in shapshot tests to avoid problems
    /// with absolute path diffs
    show_wave_source: bool,
    wanted_timescale: Timescale,
    gesture_start_location: Option<emath::Pos2>,
    show_url_entry: bool,
    show_rename_item: bool,
    filter_focused: bool,

    /// The draw commands for every signal currently selected
    // For performance reasons, these need caching so we have them in a RefCell for interior
    // mutability
    draw_data: RefCell<Option<CachedDrawData>>,

    // Egui requires a place to store text field content between frames
    url: RefCell<String>,
    command_prompt_text: RefCell<String>,
    last_canvas_rect: RefCell<Option<Rect>>,
    signal_filter: RefCell<String>,
    item_renaming_string: RefCell<String>,
    item_renaming_idx: RefCell<Option<usize>>,
}

impl State {
    fn new(args: StartupParams) -> Result<State> {
        let (sender, receiver) = channel();

        // Basic translators that we can load quickly
        let translators = TranslatorList::new(
            vec![
                Box::new(translation::BitTranslator {}),
                Box::new(translation::HexTranslator {}),
                Box::new(translation::OctalTranslator {}),
                Box::new(translation::UnsignedTranslator {}),
                Box::new(translation::SignedTranslator {}),
                Box::new(translation::GroupingBinaryTranslator {}),
                Box::new(translation::BinaryTranslator {}),
                Box::new(translation::ASCIITranslator {}),
                Box::new(translation::SinglePrecisionTranslator {}),
                Box::new(translation::DoublePrecisionTranslator {}),
                Box::new(translation::HalfPrecisionTranslator {}),
                Box::new(translation::BFloat16Translator {}),
                Box::new(translation::Posit32Translator {}),
                Box::new(translation::Posit16Translator {}),
                Box::new(translation::Posit8Translator {}),
                Box::new(translation::PositQuire8Translator {}),
                Box::new(translation::PositQuire16Translator {}),
                Box::new(translation::E5M2Translator {}),
                Box::new(translation::E4M3Translator {}),
                Box::new(translation::RiscvTranslator {}),
                Box::new(translation::LebTranslator {}),
            ],
            vec![
                Box::new(translation::clock::ClockTranslator::new()),
                Box::new(translation::StringTranslator {}),
            ],
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

        // load config
        let config = config::SurferConfig::new().with_context(|| "Failed to load config file")?;
        let mut result = State {
            config,
            vcd: None,
            count: None,
            translators,
            msg_sender: sender,
            msg_receiver: receiver,
            vcd_progress: None,
            blacklisted_translators: HashSet::new(),
            command_prompt: command_prompt::CommandPrompt {
                visible: false,
                expanded: String::from(""),
                suggestions: vec![],
            },
            context: None,
            show_about: false,
            show_keys: false,
            wanted_timescale: Timescale::Unit,
            gesture_start_location: None,
            show_url_entry: false,
            show_rename_item: false,
            show_wave_source: true,
            filter_focused: false,
            url: RefCell::new(String::new()),
            command_prompt_text: RefCell::new(String::new()),
            draw_data: RefCell::new(None),
            last_canvas_rect: RefCell::new(None),
            signal_filter: RefCell::new(String::new()),
            item_renaming_string: RefCell::new(String::new()),
            item_renaming_idx: RefCell::new(None),
        };

        match args.vcd {
            Some(WaveSource::Url(url)) => result.load_vcd_from_url(url),
            Some(WaveSource::File(file)) => result.load_vcd_from_file(file).unwrap(),
            Some(WaveSource::DragAndDrop(_)) => {
                error!("Attempted to load from drag and drop at startup (how?)")
            }
            None => {}
        }

        Ok(result)
    }

    fn load_vcd_from_file(&mut self, vcd_filename: Utf8PathBuf) -> Result<()> {
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

        self.load_vcd(WaveSource::File(vcd_filename), file, total_bytes);

        Ok(())
    }

    fn load_vcd_from_dropped(&mut self, file: DroppedFile) -> Result<()> {
        info!("Got a dropped file");

        let filename = file.path.and_then(|p| Utf8PathBuf::try_from(p).ok());
        let bytes = file
            .bytes
            .ok_or_else(|| anyhow!("Dropped a file with no bytes"))?;

        let total_bytes = bytes.len();

        self.load_vcd(
            WaveSource::DragAndDrop(filename),
            VecDeque::from_iter(bytes.into_iter().cloned()),
            Some(total_bytes as u64),
        );
        Ok(())
    }

    fn load_vcd_from_url(&mut self, url: String) {
        let sender = self.msg_sender.clone();
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
                Ok(b) => sender.send(Message::FileDownloaded(url, b)),
                Err(e) => sender.send(Message::Error(e)),
            }
            .unwrap();
        };
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);

        self.vcd_progress = Some(LoadProgress::Downloading(url_))
    }

    fn load_vcd(
        &mut self,
        source: WaveSource,
        reader: impl Read + Send + 'static,
        total_bytes: Option<u64>,
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

        let sender = self.msg_sender.clone();

        perform_work(move || {
            let result = parse_vcd(reader)
                .map_err(|e| anyhow!("{e}"))
                .with_context(|| format!("Failed to parse VCD file: {source}"));

            match result {
                Ok(vcd) => sender
                    .send(Message::VcdLoaded(source, Box::new(vcd)))
                    .unwrap(),
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });

        info!("Setting VCD progress");
        self.vcd_progress = Some(LoadProgress::Loading(total_bytes, progress_bytes));
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
                self.invalidate_draw_commands();
                let Some(vcd) = self.vcd.as_mut() else { return };
                if let Some(id) = descriptor.resolve(vcd) {
                    vcd.add_signal(&self.translators, id)
                }
            }
            Message::AddDivider(name) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                vcd.displayed_items
                    .push(DisplayedItem::Divider(DisplayedDivider {
                        color: None,
                        background_color: None,
                        name,
                    }));
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
                    self.invalidate_draw_commands();
                }
            }
            Message::AddCount(digit) => {
                if let Some(count) = &mut self.count {
                    count.push(digit);
                } else {
                    self.count = Some(digit.to_string())
                }
            }
            Message::InvalidateCount => self.count = None,
            Message::FocusItem(idx) => {
                let Some(vcd) = self.vcd.as_mut() else { return };

                let visible_signals_len = vcd.displayed_items.len();
                if visible_signals_len > 0 && idx < visible_signals_len {
                    vcd.focused_item = Some(idx);
                } else {
                    error!(
                        "Can not focus signal {idx} because only {visible_signals_len} signals are visible.",
                    );
                }
            }
            Message::UnfocusItem => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                vcd.focused_item = None;
            }
            Message::RenameItem(vidx) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                self.show_rename_item = true;
                *self.item_renaming_idx.borrow_mut() = Some(vidx);
                *self.item_renaming_string.borrow_mut() =
                    vcd.displayed_items.get(vidx).unwrap().name();
            }
            Message::MoveFocus(direction, count) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                let visible_signals_len = vcd.displayed_items.len();
                if visible_signals_len > 0 {
                    self.count = None;
                    match direction {
                        MoveDir::Up => {
                            vcd.focused_item = vcd
                                .focused_item
                                .map_or(Some(visible_signals_len - 1), |focused| {
                                    Some(focused - count.clamp(0, focused))
                                })
                        }
                        MoveDir::Down => {
                            vcd.focused_item = vcd.focused_item.map_or(
                                Some(vcd.scroll + (count - 1).clamp(0, visible_signals_len - 1)),
                                |focused| Some((focused + count).clamp(0, visible_signals_len - 1)),
                            );
                        }
                    }
                }
            }
            Message::SetVerticalScroll(position) => {
                if let Some(vcd) = &mut self.vcd {
                    vcd.scroll = position.clamp(0, vcd.displayed_items.len() - 1);
                }
            }
            Message::VerticalScroll(direction, count) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                match direction {
                    MoveDir::Down => {
                        if vcd.scroll + count < vcd.displayed_items.len() {
                            vcd.scroll += count;
                        } else {
                            vcd.scroll = vcd.displayed_items.len() - 1;
                        }
                    }
                    MoveDir::Up => {
                        if vcd.scroll > count {
                            vcd.scroll -= count;
                        } else {
                            vcd.scroll = 0;
                        }
                    }
                }
            }
            Message::RemoveItem(idx, count) => {
                self.invalidate_draw_commands();

                let Some(vcd) = self.vcd.as_mut() else { return };
                for _ in 0..count {
                    let visible_signals_len = vcd.displayed_items.len();
                    if let Some(DisplayedItem::Cursor(cursor)) = vcd.displayed_items.get(idx) {
                        vcd.cursors.remove(&cursor.idx);
                    }
                    if visible_signals_len > 0 && idx <= (visible_signals_len - 1) {
                        vcd.displayed_items.remove(idx);
                        if let Some(focused) = vcd.focused_item {
                            if focused == idx {
                                if (idx > 0) && (idx == (visible_signals_len - 1)) {
                                    // if the end of list is selected
                                    vcd.focused_item = Some(idx - 1);
                                }
                            } else {
                                if idx < focused {
                                    vcd.focused_item = Some(focused - 1)
                                }
                            }
                            if vcd.displayed_items.is_empty() {
                                vcd.focused_item = None;
                            }
                        }
                    }
                }
                vcd.compute_signal_display_names();
            }
            Message::MoveFocusedItem(direction, count) => {
                self.invalidate_draw_commands();
                let Some(vcd) = self.vcd.as_mut() else { return };
                if let Some(idx) = vcd.focused_item {
                    let visible_signals_len = vcd.displayed_items.len();
                    if visible_signals_len > 0 {
                        match direction {
                            MoveDir::Up => {
                                for i in (idx
                                    .saturating_sub(count - 1)
                                    .clamp(1, visible_signals_len - 1)
                                    ..=idx)
                                    .rev()
                                {
                                    vcd.displayed_items.swap(i, i - 1);
                                    vcd.focused_item = Some(i - 1);
                                }
                            }
                            MoveDir::Down => {
                                for i in idx..(idx + count).clamp(0, visible_signals_len - 1) {
                                    vcd.displayed_items.swap(i, i + 1);
                                    vcd.focused_item = Some(i + 1);
                                }
                            }
                        }
                    }
                }
            }
            Message::CanvasScroll { delta } => {
                self.invalidate_draw_commands();
                self.handle_canvas_scroll(delta);
            }
            Message::CanvasZoom {
                delta,
                mouse_ptr_timestamp,
            } => {
                self.invalidate_draw_commands();
                self.vcd
                    .as_mut()
                    .map(|vcd| vcd.handle_canvas_zoom(mouse_ptr_timestamp, delta as f64));
            }
            Message::ZoomToFit => {
                self.invalidate_draw_commands();
                self.zoom_to_fit();
            }
            Message::GoToEnd => {
                self.invalidate_draw_commands();
                self.scroll_to_end();
            }
            Message::GoToStart => {
                self.invalidate_draw_commands();
                self.scroll_to_start();
            }
            Message::SetTimeScale(timescale) => {
                self.invalidate_draw_commands();
                self.wanted_timescale = timescale;
            }
            Message::ZoomToRange { start, end } => {
                if let Some(vcd) = &mut self.vcd {
                    vcd.viewport.curr_left = start;
                    vcd.viewport.curr_right = end;
                }
                self.invalidate_draw_commands();
            }
            Message::SignalFormatChange(descriptor, format) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                if let Some(idx) = &descriptor.0.resolve(vcd) {
                    let path = descriptor.1;

                    if self.translators.all_translator_names().contains(&&format) {
                        *vcd.signal_format
                            .entry((idx.clone(), path.clone()))
                            .or_default() = format;

                        if path.is_empty() {
                            let signal = vcd.inner.signal_from_signal_idx(*idx);
                            let translator = vcd
                                .signal_translator((idx.clone(), path.clone()), &self.translators);
                            let new_info = translator
                                .signal_info(&signal, &vcd.signal_name(*idx))
                                .unwrap();

                            for item in &mut vcd.displayed_items {
                                match item {
                                    DisplayedItem::Signal(signal) => {
                                        if &signal.idx == idx {
                                            signal.info = new_info;
                                            break;
                                        }
                                    }
                                    DisplayedItem::Divider(_) => {}
                                    DisplayedItem::Cursor(_) => {}
                                }
                            }
                        }
                        self.invalidate_draw_commands();
                    } else {
                        println!("WARN: No translator {format}")
                    }
                }
            }
            Message::ItemColorChange(vidx, color_name) => {
                let Some(vcd) = self.vcd.as_mut() else {
                    return;
                };

                if let Some(idx) = vidx.or(vcd.focused_item) {
                    vcd.displayed_items[idx].set_color(color_name);
                };
            }
            Message::ItemNameChange(vidx, name) => {
                let Some(vcd) = self.vcd.as_mut() else {
                    return;
                };

                if let Some(idx) = vidx.or(vcd.focused_item) {
                    vcd.displayed_items[idx].set_name(name);
                };
            }
            Message::ItemBackgroundColorChange(vidx, color_name) => {
                let Some(vcd) = self.vcd.as_mut() else {
                    return;
                };

                if let Some(idx) = vidx.or(vcd.focused_item) {
                    vcd.displayed_items[idx].set_background_color(color_name)
                };
            }
            Message::ResetSignalFormat(idx) => {
                self.invalidate_draw_commands();
                self.vcd.as_mut().map(|vcd| vcd.signal_format.remove(&idx));
            }
            Message::CursorSet(new) => {
                if let Some(vcd) = self.vcd.as_mut() {
                    vcd.cursor = Some(new)
                }
            }
            Message::LoadVcd(filename) => {
                self.load_vcd_from_file(filename).ok();
            }
            Message::LoadVcdFromUrl(url) => {
                self.load_vcd_from_url(url);
            }
            Message::FileDropped(dropped_file) => {
                self.load_vcd_from_dropped(dropped_file)
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
                    displayed_items: vec![],
                    signals_to_ids: HashMap::new(),
                    scopes_to_ids: HashMap::new(),
                    ids_to_fullnames: HashMap::new(),
                    viewport: Viewport::new(0., num_timestamps.clone().to_f64().unwrap()),
                    signal_format: HashMap::new(),
                    num_timestamps,
                    cursor: None,
                    cursors: HashMap::new(),
                    focused_item: None,
                    default_signal_name_type: self.config.default_signal_name_type,
                    scroll: 0,
                };
                new_vcd.initialize_signal_scope_maps();

                // Must clone timescale before consuming new_vcd
                self.wanted_timescale = new_vcd.inner.metadata.timescale.1;
                self.vcd = Some(new_vcd);
                self.vcd_progress = None;
                info!("Done setting up VCD file");
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
                self.config.layout.show_hierarchy = !self.config.layout.show_hierarchy;
            }
            Message::ToggleMenu => self.config.layout.show_menu = !self.config.layout.show_menu,
            Message::ShowCommandPrompt(new_visibility) => {
                if !new_visibility {
                    *self.command_prompt_text.borrow_mut() = "".to_string();
                    self.command_prompt.suggestions = vec![];
                    self.command_prompt.expanded = "".to_string();
                }
                self.command_prompt.visible = new_visibility;
            }
            Message::FileDownloaded(url, bytes) => {
                let size = bytes.len() as u64;
                self.load_vcd(WaveSource::Url(url), bytes.reader(), Some(size))
            }
            Message::ReloadConfig => {
                // FIXME think about a structured way to collect errors
                if let Ok(config) =
                    config::SurferConfig::new().with_context(|| "Failed to load config file")
                {
                    self.config = config;
                    if let Some(ctx) = &self.context {
                        ctx.set_visuals(self.get_visuals())
                    }
                }
            }
            Message::SetClockHighlightType(new_type) => {
                self.config.default_clock_highlight_type = new_type
            }
            Message::SetCursorPosition(idx) => {
                let Some(vcd) = self.vcd.as_mut() else {
                    return;
                };
                let Some(location) = &vcd.cursor else {
                    return;
                };
                if vcd
                    .displayed_items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayedItem::Cursor(cursor) => {
                            if cursor.idx == idx {
                                Some(cursor)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .next()
                    .is_none()
                {
                    let cursor = DisplayedCursor {
                        color: None,
                        background_color: None,
                        name: format!("Cursor {idx}"),
                        idx,
                    };
                    vcd.displayed_items.push(DisplayedItem::Cursor(cursor));
                }
                vcd.cursors.insert(idx, location.clone());
            }

            Message::GoToCursorPosition(idx) => {
                let Some(vcd) = self.vcd.as_mut() else {
                    return;
                };
                if let Some(cursor) = vcd.cursors.get(&idx) {
                    let center_point = cursor.to_f64().unwrap();
                    let half_width = (vcd.viewport.curr_right - vcd.viewport.curr_left) / 2.;

                    vcd.viewport.curr_left = center_point - half_width;
                    vcd.viewport.curr_right = center_point + half_width;

                    self.invalidate_draw_commands();
                }
            }

            Message::ChangeSignalNameType(vidx, name_type) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                // checks if vidx is Some then use that, else try focused signal
                if let Some(idx) = vidx.or(vcd.focused_item) {
                    if vcd.displayed_items.len() > idx {
                        if let DisplayedItem::Signal(signal) = &mut vcd.displayed_items[idx] {
                            signal.display_name_type = name_type;
                            vcd.compute_signal_display_names();
                        }
                    }
                }
            }
            Message::ForceSignalNameTypes(name_type) => {
                let Some(vcd) = self.vcd.as_mut() else { return };
                for signal in &mut vcd.displayed_items {
                    if let DisplayedItem::Signal(signal) = signal {
                        signal.display_name_type = name_type;
                    }
                }
                vcd.default_signal_name_type = name_type;
                vcd.compute_signal_display_names();
            }
            Message::CommandPromptClear => {
                *self.command_prompt_text.borrow_mut() = "".to_string();
                self.command_prompt.expanded = "".to_string();
                self.command_prompt.suggestions = vec![];
            }
            Message::CommandPromptUpdate {
                expanded,
                suggestions,
            } => {
                self.command_prompt.expanded = expanded;
                self.command_prompt.suggestions = suggestions;
            }
            Message::OpenFileDialog => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(path) = FileDialog::new()
                    .set_title("Open waveform file")
                    .add_filter("VCD-files (*.vcd)", &["vcd"])
                    .add_filter("All files", &["*"])
                    .pick_file()
                {
                    self.load_vcd_from_file(camino::Utf8PathBuf::from_path_buf(path).unwrap())
                        .ok();
                }
            }
            Message::SetAboutVisible(s) => self.show_about = s,
            Message::SetKeyHelpVisible(s) => self.show_keys = s,
            Message::SetUrlEntryVisible(s) => self.show_url_entry = s,
            Message::SetRenameItemVisible(s) => self.show_rename_item = s,
            Message::SetDragStart(pos) => self.gesture_start_location = pos,
            Message::SetFilterFocused(s) => self.filter_focused = s,
            Message::Exit | Message::ToggleFullscreen => {} // Handled in eframe::update
        }
    }

    fn handle_async_messages(&mut self) {
        let mut msgs = vec![];
        loop {
            match self.msg_receiver.try_recv() {
                Ok(msg) => msgs.push(msg),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    trace!("Message sender disconnected");
                    break;
                }
            }
        }

        while let Some(msg) = msgs.pop() {
            self.update(msg);
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
            let scroll_step = -(vcd.viewport.curr_right - vcd.viewport.curr_left) / (50. * 20.);

            let target_left = &vcd.viewport.curr_left + scroll_step * delta.y as f64;
            let target_right = &vcd.viewport.curr_right + scroll_step * delta.y as f64;

            vcd.viewport.curr_left = target_left;
            vcd.viewport.curr_right = target_right;
        }
    }

    pub fn scroll_to_start(&mut self) {
        if let Some(vcd) = &mut self.vcd {
            let width = vcd.viewport.curr_right - vcd.viewport.curr_left;

            vcd.viewport.curr_left = 0.0;
            vcd.viewport.curr_right = width;
        }
    }

    pub fn scroll_to_end(&mut self) {
        if let Some(vcd) = &mut self.vcd {
            let end_point = vcd.num_timestamps.clone().to_f64().unwrap();
            let width = vcd.viewport.curr_right - vcd.viewport.curr_left;

            vcd.viewport.curr_left = end_point - width;
            vcd.viewport.curr_right = end_point;
        }
    }

    pub fn set_center_point(&mut self, center: BigInt) {
        if let Some(vcd) = &mut self.vcd {
            let center_point = center.to_f64().unwrap();
            let half_width = (vcd.viewport.curr_right - vcd.viewport.curr_left) / 2.;

            vcd.viewport.curr_left = center_point - half_width;
            vcd.viewport.curr_right = center_point + half_width;
        }
    }
    pub fn zoom_to_fit(&mut self) {
        if let Some(vcd) = &mut self.vcd {
            vcd.viewport.curr_left = 0.0;
            vcd.viewport.curr_right = vcd.num_timestamps.clone().to_f64().unwrap();
        }
    }

    pub fn get_visuals(&self) -> Visuals {
        let widget_style = WidgetVisuals {
            bg_fill: self.config.theme.secondary_ui_color.background,
            fg_stroke: Stroke {
                color: self.config.theme.secondary_ui_color.foreground,
                width: 1.0,
            },
            weak_bg_fill: self.config.theme.secondary_ui_color.background,
            bg_stroke: Stroke {
                color: self.config.theme.border_color,
                width: 1.0,
            },
            rounding: Rounding::same(2.),
            expansion: 0.0,
        };

        Visuals {
            override_text_color: Some(self.config.theme.foreground),
            extreme_bg_color: self.config.theme.secondary_ui_color.background,
            panel_fill: self.config.theme.secondary_ui_color.background,
            window_fill: self.config.theme.primary_ui_color.background,
            window_rounding: Rounding::ZERO,
            menu_rounding: Rounding::ZERO,
            window_stroke: Stroke {
                width: 1.0,
                color: self.config.theme.border_color,
            },
            selection: Selection {
                bg_fill: self.config.theme.selected_elements_colors.background,
                stroke: Stroke {
                    color: self.config.theme.selected_elements_colors.foreground,
                    width: 1.0,
                },
            },
            widgets: Widgets {
                noninteractive: widget_style,
                inactive: widget_style,
                hovered: widget_style,
                active: widget_style,
                open: widget_style,
                ..Default::default()
            },
            ..Visuals::dark()
        }
    }

    fn get_count(&self) -> usize {
        if let Some(count) = &self.count {
            usize::from_str_radix(count, 10).unwrap_or(1)
        } else {
            1
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
        mouse_ptr_timestamp: Option<f64>,
        delta: f64,
    ) {
        // Zoom or scroll
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self.viewport;

        let (target_left, target_right) = match mouse_ptr_timestamp {
            Some(mouse_location) => (
                (left - mouse_location) / delta + mouse_location,
                (right - mouse_location) / delta + mouse_location,
            ),
            None => {
                let mid_point = (right + left) * 0.5;
                let offset = (right - left) * delta * 0.5;

                (mid_point - offset, mid_point + offset)
            }
        };

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
                    let fullname = format!("{}.{}", full_scope_name, signal_name);
                    vcd.signals_to_ids.insert(fullname.clone(), signal);
                    vcd.ids_to_fullnames.insert(signal, fullname);
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

        self.displayed_items
            .push(DisplayedItem::Signal(DisplayedSignal {
                idx: sidx,
                info,
                color: None,
                background_color: None,
                display_name: signal.name().clone(),
                display_name_type: self.default_signal_name_type,
            }));
        self.compute_signal_display_names();
    }

    pub fn compute_signal_display_names(&mut self) {
        let full_names = self
            .displayed_items
            .iter()
            .filter_map(|item| match item {
                DisplayedItem::Signal(idx) => Some(idx),
                _ => None,
            })
            .map(|sig| sig.idx)
            .unique()
            .map(|idx| {
                self.ids_to_fullnames
                    .get(&idx)
                    .map(|name| name.clone())
                    .unwrap_or_else(|| self.inner.signal_from_signal_idx(idx).name())
            })
            .collect_vec();

        for item in &mut self.displayed_items {
            match item {
                DisplayedItem::Signal(signal) => {
                    let local_name = self.inner.signal_from_signal_idx(signal.idx).name();
                    signal.display_name = match signal.display_name_type {
                        SignalNameType::Local => local_name,
                        SignalNameType::Global => self
                            .ids_to_fullnames
                            .get(&signal.idx)
                            .unwrap_or(&local_name)
                            .clone(),
                        SignalNameType::Unique => {
                            /// This function takes a full signal name and a list of other
                            /// full signal names and returns a minimal unique signal name.
                            /// It takes scopes from the back of the signal until the name is unique.
                            fn unique(signal: String, signals: &[String]) -> String {
                                // if the full signal name is very short just return it
                                if signal.len() < 20 {
                                    return signal;
                                }

                                let split_this =
                                    signal.split('.').map(|p| p.to_string()).collect_vec();
                                let split_signals = signals
                                    .iter()
                                    .filter(|&s| *s != signal)
                                    .map(|s| s.split('.').map(|p| p.to_string()).collect_vec())
                                    .collect_vec();

                                fn take_front(s: &Vec<String>, l: usize) -> String {
                                    if l == 0 {
                                        s.last().unwrap().clone()
                                    } else if l < s.len() - 1 {
                                        format!("...{}", s.iter().rev().take(l + 1).rev().join("."))
                                    } else {
                                        s.join(".")
                                    }
                                }

                                let mut l = 0;
                                while split_signals
                                    .iter()
                                    .map(|s| take_front(s, l))
                                    .contains(&take_front(&split_this, l))
                                {
                                    l += 1;
                                }
                                take_front(&split_this, l)
                            }

                            let full_name = self.ids_to_fullnames.get(&signal.idx).unwrap().clone();
                            unique(full_name, &full_names)
                        }
                    };
                }
                DisplayedItem::Divider(_) => {}
                DisplayedItem::Cursor(_) => {}
            }
        }
    }
}
