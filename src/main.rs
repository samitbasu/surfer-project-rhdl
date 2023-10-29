mod benchmark;
mod command_prompt;
mod commands;
mod config;
mod mousegestures;
mod fast_wave_container;
mod signal_canvas;
#[cfg(test)]
mod tests;
mod translation;
mod util;
mod view;
mod viewport;
mod wasm_util;
mod wave_container;

use bytes::Buf;
use bytes::Bytes;
use camino::Utf8PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
use color_eyre::eyre::anyhow;
use color_eyre::eyre::Context;
use color_eyre::Result;
use derivative::Derivative;
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
use fastwave_backend::Timescale;
#[cfg(not(target_arch = "wasm32"))]
use fern::colors::ColoredLevelConfig;
use futures_util::FutureExt;
use futures_util::TryFutureExt;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use itertools::Itertools;
use log::error;
use log::info;
use log::trace;
use log::warn;
use num::bigint::ToBigInt;
use num::BigInt;
use num::FromPrimitive;
use num::ToPrimitive;
use progress_streams::ProgressReader;
use regex::Regex;
#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;
use serde::Deserialize;
use translation::spade::SpadeTranslator;
use translation::SignalInfo;
use translation::TranslationPreference;
use translation::Translator;
use translation::TranslatorList;
use viewport::Viewport;
use wasm_util::perform_work;
use wave_container::FieldRef;
use wave_container::ModuleRef;
use wave_container::SignalMeta;
use wave_container::SignalRef;
use wave_container::WaveContainer;

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
        initial_window_size: Some(egui::vec2(
            state.config.layout.window_width as f32,
            state.config.layout.window_height as f32,
        )),
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
    signal_ref: SignalRef,
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

    pub fn display_name(&self) -> String {
        match self {
            DisplayedItem::Signal(signal) => signal.display_name.clone(),
            DisplayedItem::Divider(divider) => divider.name.clone(),
            DisplayedItem::Cursor(cursor) => {
                format!("{idx}: {name}", idx = cursor.idx, name = cursor.name)
            }
        }
    }

    pub fn set_name(&mut self, name: String) {
        match self {
            DisplayedItem::Signal(_) => {
                warn!("Renaming signal");
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

pub struct WaveData {
    inner: WaveContainer,
    source: WaveSource,
    active_module: Option<ModuleRef>,
    /// Root items (signals, dividers, ...) to display
    displayed_items: Vec<DisplayedItem>,
    viewport: Viewport,
    num_timestamps: BigInt,
    /// Name of the translator used to translate this trace
    signal_format: HashMap<FieldRef, String>,
    cursor: Option<BigInt>,
    cursors: HashMap<u8, BigInt>,
    focused_item: Option<usize>,
    default_signal_name_type: SignalNameType,
    scroll: usize,
}

impl WaveData {
    pub fn update_with(
        mut self,
        new_waves: Box<WaveContainer>,
        source: WaveSource,
        num_timestamps: BigInt,
        wave_viewport: Viewport,
        translators: &TranslatorList,
    ) -> WaveData {
        let active_module = self
            .active_module
            .take()
            .filter(|m| new_waves.module_exists(m));
        let display_items = self
            .displayed_items
            .drain(..)
            .filter(|i| match i {
                DisplayedItem::Signal(s) => new_waves.signal_exists(&s.signal_ref),
                DisplayedItem::Divider(_) => true,
                DisplayedItem::Cursor(_) => true,
            })
            .collect::<Vec<_>>();
        let mut nested_format = self
            .signal_format
            .iter()
            .filter(|&(field_ref, _)| !field_ref.field.is_empty())
            .map(|(x, y)| (x.clone(), y.clone()))
            .collect::<HashMap<_, _>>();
        let signal_format = self
            .signal_format
            .drain()
            .filter(|(field_ref, candidate)| {
                display_items.iter().any(|di| match di {
                    DisplayedItem::Signal(DisplayedSignal { signal_ref, .. }) => {
                        let Ok(meta) = new_waves.signal_meta(signal_ref) else {
                            return false;
                        };
                        field_ref.field.is_empty()
                            && *signal_ref == field_ref.root
                            && translators.is_valid_translator(&meta, candidate.as_str())
                    }
                    _ => false,
                })
            })
            .collect();
        let mut new_wave = WaveData {
            inner: *new_waves,
            source,
            active_module,
            displayed_items: display_items,
            viewport: self.viewport.clone().clip_to(&wave_viewport),
            signal_format,
            num_timestamps,
            cursor: self.cursor.clone(),
            cursors: self.cursors.clone(),
            focused_item: self.focused_item,
            default_signal_name_type: self.default_signal_name_type,
            scroll: self.scroll,
        };
        nested_format.retain(|nested, _| {
            let Some(signal_ref) = new_wave
                .displayed_items
                .iter()
                .find_map(|di| match di {
                    DisplayedItem::Signal(DisplayedSignal { signal_ref, .. }) => Some(signal_ref),
                    _ => None,
                })
            else {
                return false;
            };
            let meta = new_wave.inner.signal_meta(&nested.root).unwrap();
            new_wave
                .signal_translator(
                    &FieldRef {
                        root: signal_ref.clone(),
                        field: vec![],
                    },
                    translators,
                )
                .signal_info(&meta)
                .map(|info| info.has_subpath(&nested.field))
                .unwrap_or(false)
        });
        new_wave.signal_format.extend(nested_format);
        new_wave
    }
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

#[derive(Debug, PartialEq)]
pub enum SignalFilterType {
    Fuzzy,
    Regex,
    Start,
    Contain,
}

impl std::fmt::Display for SignalFilterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalFilterType::Fuzzy => write!(f, "Fuzzy"),
            SignalFilterType::Regex => write!(f, "Regular expression"),
            SignalFilterType::Start => write!(f, "Signal starts with"),
            SignalFilterType::Contain => write!(f, "Signal contains"),
        }
    }
}

impl SignalFilterType {
    fn is_match(&self, signal_name: &str, filter: &str) -> bool {
        match self {
            SignalFilterType::Fuzzy => {
                let matcher = SkimMatcherV2::default();
                matcher.fuzzy_match(signal_name, filter).is_some()
            }
            SignalFilterType::Contain => signal_name.contains(filter),
            SignalFilterType::Start => signal_name.starts_with(filter),
            SignalFilterType::Regex => {
                if let Ok(regex) = Regex::new(filter) {
                    regex.is_match(signal_name)
                } else {
                    false
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum OpenMode {
    Open,
    Switch,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub enum Message {
    SetActiveScope(ModuleRef),
    AddSignal(SignalRef),
    AddModule(ModuleRef),
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
    SignalFormatChange(FieldRef, String),
    ItemColorChange(Option<usize>, Option<String>),
    ItemBackgroundColorChange(Option<usize>, Option<String>),
    ItemNameChange(Option<usize>, String),
    ChangeSignalNameType(Option<usize>, SignalNameType),
    ForceSignalNameTypes(SignalNameType),
    SetClockHighlightType(ClockHighlightType),
    // Reset the translator for this signal back to default. Sub-signals,
    // i.e. those with the signal idx and a shared path are also reset
    ResetSignalFormat(FieldRef),
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
    WavesLoaded(WaveSource, Box<WaveContainer>, bool),
    Error(color_eyre::eyre::Error),
    TranslatorLoaded(#[derivative(Debug = "ignore")] Box<dyn Translator + Send>),
    /// Take note that the specified translator errored on a `translates` call on the
    /// specified signal
    BlacklistTranslator(SignalRef, String),
    ToggleSidePanel,
    ShowCommandPrompt(bool),
    FileDropped(DroppedFile),
    FileDownloaded(String, Bytes, bool),
    ReloadConfig,
    ReloadWaveform,
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
    OpenFileDialog(OpenMode),
    SetAboutVisible(bool),
    SetKeyHelpVisible(bool),
    SetGestureHelpVisible(bool),
    SetUrlEntryVisible(bool),
    SetRenameItemVisible(bool),
    SetDragStart(Option<Pos2>),
    SetFilterFocused(bool),
    SetSignalFilterType(SignalFilterType),
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
    pub draw_commands: HashMap<FieldRef, signal_canvas::DrawingCommands>,
    pub clock_edges: Vec<f32>,
}

pub struct State {
    config: config::SurferConfig,
    waves: Option<WaveData>,
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
    blacklisted_translators: HashSet<(SignalRef, String)>,
    /// Buffer for the command input
    command_prompt: command_prompt::CommandPrompt,

    /// The context to egui, we need this to change the visual settings when the config is reloaded
    context: Option<eframe::egui::Context>,

    show_about: bool,
    show_keys: bool,
    show_gestures: bool,
    /// Hide the wave source. For now, this is only used in shapshot tests to avoid problems
    /// with absolute path diffs
    show_wave_source: bool,
    wanted_timescale: Timescale,
    gesture_start_location: Option<emath::Pos2>,
    show_url_entry: bool,
    signal_filter_focused: bool,
    signal_filter_type: SignalFilterType,
    rename_target: Option<usize>,

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
            waves: None,
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
            show_gestures: false,
            wanted_timescale: Timescale::Unit,
            gesture_start_location: None,
            show_url_entry: false,
            rename_target: None,
            show_wave_source: true,
            signal_filter_focused: false,
            signal_filter_type: SignalFilterType::Fuzzy,
            url: RefCell::new(String::new()),
            command_prompt_text: RefCell::new(String::new()),
            draw_data: RefCell::new(None),
            last_canvas_rect: RefCell::new(None),
            signal_filter: RefCell::new(String::new()),
            item_renaming_string: RefCell::new(String::new()),
        };

        match args.vcd {
            Some(WaveSource::Url(url)) => result.load_vcd_from_url(url, false),
            Some(WaveSource::File(file)) => result.load_vcd_from_file(file, false).unwrap(),
            Some(WaveSource::DragAndDrop(_)) => {
                error!("Attempted to load from drag and drop at startup (how?)")
            }
            None => {}
        }

        Ok(result)
    }

    fn load_vcd_from_file(&mut self, vcd_filename: Utf8PathBuf, keep_signals: bool) -> Result<()> {
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

    fn load_vcd_from_dropped(&mut self, file: DroppedFile, keep_signals: bool) -> Result<()> {
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
            keep_signals,
        );
        Ok(())
    }

    fn load_vcd_from_url(&mut self, url: String, keep_signals: bool) {
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
                Ok(b) => sender.send(Message::FileDownloaded(url, b, keep_signals)),
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

        let sender = self.msg_sender.clone();

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
        self.vcd_progress = Some(LoadProgress::Loading(total_bytes, progress_bytes));
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::SetActiveScope(sig) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                // TODO: Perhaps we should verify that the scope exists here
                waves.active_module = Some(sig)
            }
            Message::AddSignal(sig) => {
                self.invalidate_draw_commands();
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                waves.add_signal(&self.translators, &sig)
            }
            Message::AddDivider(name) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                waves
                    .displayed_items
                    .push(DisplayedItem::Divider(DisplayedDivider {
                        color: None,
                        background_color: None,
                        name,
                    }));
            }
            Message::AddModule(module) => {
                let Some(waves) = self.waves.as_mut() else {
                    warn!("Adding module without waves loaded");
                    return;
                };

                let signals = waves.inner.signals_in_module(&module);
                for signal in signals {
                    waves.add_signal(&self.translators, &signal);
                }
                self.invalidate_draw_commands();
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
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                let visible_signals_len = waves.displayed_items.len();
                if visible_signals_len > 0 && idx < visible_signals_len {
                    waves.focused_item = Some(idx);
                } else {
                    error!(
                        "Can not focus signal {idx} because only {visible_signals_len} signals are visible.",
                    );
                }
            }
            Message::UnfocusItem => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                waves.focused_item = None;
            }
            Message::RenameItem(vidx) => {
                let Some(waves) = self.waves.as_mut() else { return };
                self.rename_target = Some(vidx);
                *self.item_renaming_string.borrow_mut() =
                    waves.displayed_items.get(vidx).unwrap().name();
            }
            Message::MoveFocus(direction, count) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                let visible_signals_len = waves.displayed_items.len();
                if visible_signals_len > 0 {
                    self.count = None;
                    match direction {
                        MoveDir::Up => {
                            waves.focused_item = waves
                                .focused_item
                                .map_or(Some(visible_signals_len - 1), |focused| {
                                    Some(focused - count.clamp(0, focused))
                                })
                        }
                        MoveDir::Down => {
                            waves.focused_item = waves.focused_item.map_or(
                                Some(waves.scroll + (count - 1).clamp(0, visible_signals_len - 1)),
                                |focused| Some((focused + count).clamp(0, visible_signals_len - 1)),
                            );
                        }
                    }
                }
            }
            Message::SetVerticalScroll(position) => {
                if let Some(waves) = &mut self.waves {
                    waves.scroll = position.clamp(0, waves.displayed_items.len() - 1);
                }
            }
            Message::VerticalScroll(direction, count) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                match direction {
                    MoveDir::Down => {
                        if waves.scroll + count < waves.displayed_items.len() {
                            waves.scroll += count;
                        } else {
                            waves.scroll = waves.displayed_items.len() - 1;
                        }
                    }
                    MoveDir::Up => {
                        if waves.scroll > count {
                            waves.scroll -= count;
                        } else {
                            waves.scroll = 0;
                        }
                    }
                }
            }
            Message::RemoveItem(idx, count) => {
                self.invalidate_draw_commands();

                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                for _ in 0..count {
                    let visible_signals_len = waves.displayed_items.len();
                    if let Some(DisplayedItem::Cursor(cursor)) = waves.displayed_items.get(idx) {
                        waves.cursors.remove(&cursor.idx);
                    }
                    if visible_signals_len > 0 && idx <= (visible_signals_len - 1) {
                        waves.displayed_items.remove(idx);
                        if let Some(focused) = waves.focused_item {
                            if focused == idx {
                                if (idx > 0) && (idx == (visible_signals_len - 1)) {
                                    // if the end of list is selected
                                    waves.focused_item = Some(idx - 1);
                                }
                            } else {
                                if idx < focused {
                                    waves.focused_item = Some(focused - 1)
                                }
                            }
                            if waves.displayed_items.is_empty() {
                                waves.focused_item = None;
                            }
                        }
                    }
                }
                waves.compute_signal_display_names();
            }
            Message::MoveFocusedItem(direction, count) => {
                self.invalidate_draw_commands();
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                if let Some(idx) = waves.focused_item {
                    let visible_signals_len = waves.displayed_items.len();
                    if visible_signals_len > 0 {
                        match direction {
                            MoveDir::Up => {
                                for i in (idx
                                    .saturating_sub(count - 1)
                                    .clamp(1, visible_signals_len - 1)
                                    ..=idx)
                                    .rev()
                                {
                                    waves.displayed_items.swap(i, i - 1);
                                    waves.focused_item = Some(i - 1);
                                }
                            }
                            MoveDir::Down => {
                                for i in idx..(idx + count).clamp(0, visible_signals_len - 1) {
                                    waves.displayed_items.swap(i, i + 1);
                                    waves.focused_item = Some(i + 1);
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
                self.waves
                    .as_mut()
                    .map(|waves| waves.handle_canvas_zoom(mouse_ptr_timestamp, delta as f64));
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
                if let Some(waves) = &mut self.waves {
                    waves.viewport.curr_left = start;
                    waves.viewport.curr_right = end;
                }
                self.invalidate_draw_commands();
            }
            Message::SignalFormatChange(field, format) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if self.translators.all_translator_names().contains(&&format) {
                    *waves.signal_format.entry(field.clone()).or_default() = format;

                    if field.field.is_empty() {
                        let Ok(meta) = waves
                            .inner
                            .signal_meta(&field.root)
                            .map_err(|e| warn!("{e:#?}"))
                        else {
                            return;
                        };
                        let translator = waves.signal_translator(&field, &self.translators);
                        let new_info = translator.signal_info(&meta).unwrap();

                        for item in &mut waves.displayed_items {
                            match item {
                                DisplayedItem::Signal(disp) => {
                                    if &disp.signal_ref == &field.root {
                                        disp.info = new_info;
                                        break;
                                    }
                                }
                                DisplayedItem::Cursor(_) => {}
                                DisplayedItem::Divider(_) => {}
                            }
                        }
                    }
                    self.invalidate_draw_commands();
                } else {
                    warn!("No translator {format}")
                }
            }
            Message::ItemColorChange(vidx, color_name) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if let Some(idx) = vidx.or(waves.focused_item) {
                    waves.displayed_items[idx].set_color(color_name);
                };
            }
            Message::ItemNameChange(vidx, name) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if let Some(idx) = vidx.or(waves.focused_item) {
                    waves.displayed_items[idx].set_name(name);
                };
            }
            Message::ItemBackgroundColorChange(vidx, color_name) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if let Some(idx) = vidx.or(waves.focused_item) {
                    waves.displayed_items[idx].set_background_color(color_name)
                };
            }
            Message::ResetSignalFormat(idx) => {
                self.invalidate_draw_commands();
                self.waves
                    .as_mut()
                    .map(|waves| waves.signal_format.remove(&idx));
            }
            Message::CursorSet(new) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.cursor = Some(new)
                }
            }
            Message::LoadVcd(filename) => {
                self.load_vcd_from_file(filename, false).ok();
            }
            Message::LoadVcdFromUrl(url) => {
                self.load_vcd_from_url(url, false);
            }
            Message::FileDropped(dropped_file) => {
                self.load_vcd_from_dropped(dropped_file, false)
                    .map_err(|e| error!("{e:#?}"))
                    .ok();
            }
            Message::WavesLoaded(filename, new_waves, keep_signals) => {
                info!("VCD file loaded");
                let num_timestamps = new_waves
                    .max_timestamp()
                    .as_ref()
                    .map(|t| t.to_bigint().unwrap())
                    .unwrap_or(BigInt::from_u32(1).unwrap());
                let viewport = Viewport::new(0., num_timestamps.clone().to_f64().unwrap());

                let new_wave = if keep_signals && self.waves.is_some() {
                    self.waves.take().unwrap().update_with(
                        new_waves,
                        filename,
                        num_timestamps,
                        viewport,
                        &self.translators,
                    )
                } else {
                    WaveData {
                        inner: *new_waves,
                        source: filename,
                        active_module: None,
                        displayed_items: vec![],
                        viewport,
                        signal_format: HashMap::new(),
                        num_timestamps,
                        cursor: None,
                        cursors: HashMap::new(),
                        focused_item: None,
                        default_signal_name_type: self.config.default_signal_name_type,
                        scroll: 0,
                    }
                };
                self.invalidate_draw_commands();

                // Must clone timescale before consuming new_vcd
                self.wanted_timescale = new_wave.inner.metadata().timescale.1;
                self.waves = Some(new_wave);
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
            Message::FileDownloaded(url, bytes, keep_signals) => {
                let size = bytes.len() as u64;
                self.load_vcd(
                    WaveSource::Url(url),
                    bytes.reader(),
                    Some(size),
                    keep_signals,
                )
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
            Message::ReloadWaveform => {
                let Some(waves) = &self.waves else { return };
                match &waves.source {
                    WaveSource::File(filename) => {
                        self.load_vcd_from_file(filename.clone(), true).ok()
                    }
                    WaveSource::DragAndDrop(filename) => filename
                        .clone()
                        .and_then(|filename| self.load_vcd_from_file(filename, true).ok()),
                    WaveSource::Url(url) => {
                        self.load_vcd_from_url(url.clone(), true);
                        Some(())
                    }
                };
            }
            Message::SetClockHighlightType(new_type) => {
                self.config.default_clock_highlight_type = new_type
            }
            Message::SetCursorPosition(idx) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                let Some(location) = &waves.cursor else {
                    return;
                };
                if waves
                    .displayed_items
                    .iter()
                    .find_map(|item| match item {
                        DisplayedItem::Cursor(cursor) => {
                            if cursor.idx == idx {
                                Some(cursor)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .is_none()
                {
                    let cursor = DisplayedCursor {
                        color: None,
                        background_color: None,
                        name: format!("Cursor"),
                        idx,
                    };
                    waves.displayed_items.push(DisplayedItem::Cursor(cursor));
                }
                waves.cursors.insert(idx, location.clone());
            }

            Message::GoToCursorPosition(idx) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                if let Some(cursor) = waves.cursors.get(&idx) {
                    let center_point = cursor.to_f64().unwrap();
                    let half_width = (waves.viewport.curr_right - waves.viewport.curr_left) / 2.;

                    waves.viewport.curr_left = center_point - half_width;
                    waves.viewport.curr_right = center_point + half_width;

                    self.invalidate_draw_commands();
                }
            }

            Message::ChangeSignalNameType(vidx, name_type) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                // checks if vidx is Some then use that, else try focused signal
                if let Some(idx) = vidx.or(waves.focused_item) {
                    if waves.displayed_items.len() > idx {
                        if let DisplayedItem::Signal(signal) = &mut waves.displayed_items[idx] {
                            signal.display_name_type = name_type;
                            waves.compute_signal_display_names();
                        }
                    }
                }
            }
            Message::ForceSignalNameTypes(name_type) => {
                let Some(vcd) = self.waves.as_mut() else {
                    return;
                };
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
            Message::OpenFileDialog(mode) => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(path) = FileDialog::new()
                    .set_title("Open waveform file")
                    .add_filter("VCD-files (*.vcd)", &["vcd"])
                    .add_filter("All files", &["*"])
                    .pick_file()
                {
                    self.load_vcd_from_file(
                        camino::Utf8PathBuf::from_path_buf(path).unwrap(),
                        match mode {
                            OpenMode::Open => false,
                            OpenMode::Switch => true,
                        },
                    )
                    .ok();
                }
            }
            Message::SetAboutVisible(s) => self.show_about = s,
            Message::SetKeyHelpVisible(s) => self.show_keys = s,
            Message::SetGestureHelpVisible(s) => self.show_gestures = s,
            Message::SetUrlEntryVisible(s) => self.show_url_entry = s,
            Message::SetRenameItemVisible(_) => self.rename_target = None,
            Message::SetDragStart(pos) => self.gesture_start_location = pos,
            Message::SetFilterFocused(s) => self.signal_filter_focused = s,
            Message::SetSignalFilterType(signal_filter_type) => {
                self.signal_filter_type = signal_filter_type
            }
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
        if let Some(vcd) = &mut self.waves {
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
        if let Some(vcd) = &mut self.waves {
            let width = vcd.viewport.curr_right - vcd.viewport.curr_left;

            vcd.viewport.curr_left = 0.0;
            vcd.viewport.curr_right = width;
        }
    }

    pub fn scroll_to_end(&mut self) {
        if let Some(vcd) = &mut self.waves {
            let end_point = vcd.num_timestamps.clone().to_f64().unwrap();
            let width = vcd.viewport.curr_right - vcd.viewport.curr_left;

            vcd.viewport.curr_left = end_point - width;
            vcd.viewport.curr_right = end_point;
        }
    }

    pub fn set_center_point(&mut self, center: BigInt) {
        if let Some(waves) = &mut self.waves {
            let center_point = center.to_f64().unwrap();
            let half_width = (waves.viewport.curr_right - waves.viewport.curr_left) / 2.;

            waves.viewport.curr_left = center_point - half_width;
            waves.viewport.curr_right = center_point + half_width;
        }
    }
    pub fn zoom_to_fit(&mut self) {
        if let Some(waves) = &mut self.waves {
            waves.viewport.curr_left = 0.0;
            waves.viewport.curr_right = waves.num_timestamps.clone().to_f64().unwrap();
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

impl WaveData {
    pub fn select_preferred_translator(
        &self,
        sig: SignalMeta,
        translators: &TranslatorList,
    ) -> String {
        translators
            .all_translators()
            .iter()
            .filter_map(|t| match t.translates(&sig) {
                Ok(TranslationPreference::Prefer) => Some(t.name()),
                Ok(TranslationPreference::Yes) => None,
                Ok(TranslationPreference::No) => None,
                Err(e) => {
                    error!(
                        "Failed to check if {} translates {}\n{e:#?}",
                        t.name(),
                        sig.sig.full_path_string()
                    );
                    None
                }
            })
            .next()
            .unwrap_or(translators.default.clone())
    }

    pub fn signal_translator<'a>(
        &'a self,
        field: &FieldRef,
        translators: &'a TranslatorList,
    ) -> &'a dyn Translator {
        let translator_name = self.signal_format.get(&field).cloned().unwrap_or_else(|| {
            if field.field.is_empty() {
                self.inner
                    .signal_meta(&field.root)
                    .map(|meta| self.select_preferred_translator(meta, translators))
                    .unwrap_or_else(|e| {
                        warn!("{e:#?}");
                        translators.default.clone()
                    })
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

    pub fn add_signal(&mut self, translators: &TranslatorList, sig: &SignalRef) {
        let Ok(meta) = self
            .inner
            .signal_meta(&sig)
            .context("When adding signal")
            .map_err(|e| error!("{e:#?}"))
        else {
            return;
        };

        let translator =
            self.signal_translator(&FieldRef::without_fields(sig.clone()), translators);
        let info = translator.signal_info(&meta).unwrap();

        self.displayed_items
            .push(DisplayedItem::Signal(DisplayedSignal {
                signal_ref: sig.clone(),
                info,
                color: None,
                background_color: None,
                display_name: sig.name.clone(),
                display_name_type: self.default_signal_name_type,
            }));
        self.compute_signal_display_names();
    }

    pub fn compute_signal_display_names(&mut self) {
        let full_names = self
            .displayed_items
            .iter()
            .filter_map(|item| match item {
                DisplayedItem::Signal(signal_ref) => Some(signal_ref),
                _ => None,
            })
            .map(|sig| sig.signal_ref.full_path_string())
            .unique()
            .collect_vec();

        for item in &mut self.displayed_items {
            match item {
                DisplayedItem::Signal(signal) => {
                    let local_name = signal.signal_ref.name.clone();
                    signal.display_name = match signal.display_name_type {
                        SignalNameType::Local => local_name,
                        SignalNameType::Global => signal.signal_ref.full_path_string(),
                        SignalNameType::Unique => {
                            /// This function takes a full signal name and a list of other
                            /// full signal names and returns a minimal unique signal name.
                            /// It takes scopes from the back of the signal until the name is unique.
                            // TODO: Rewrite this to take SignalRef which already has done the
                            // `.` splitting
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

                            let full_name = signal.signal_ref.full_path_string();
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
