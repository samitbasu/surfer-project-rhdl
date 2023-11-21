mod benchmark;
mod clock_highlighting;
mod command_prompt;
mod config;
mod cursor;
mod displayed_item;
mod fast_wave_container;
mod help;
mod keys;
mod logs;
mod menus;
mod message;
mod mousegestures;
mod signal_canvas;
mod signal_filter;
mod signal_name_type;
#[cfg(test)]
mod tests;
mod time;
mod translation;
mod util;
mod view;
mod viewport;
mod wasm_util;
mod wave_container;
mod wave_data;
mod wave_source;

use bytes::Buf;
use camino::Utf8PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
use color_eyre::eyre::Context;
use color_eyre::Result;
use command_prompt::get_parser;
use config::SurferConfig;
use displayed_item::DisplayedItem;
#[cfg(not(target_arch = "wasm32"))]
use eframe::egui;
use eframe::egui::style::Selection;
use eframe::egui::style::WidgetVisuals;
use eframe::egui::style::Widgets;
use eframe::egui::Visuals;
use eframe::emath;
use eframe::epaint::Rect;
use eframe::epaint::Rounding;
use eframe::epaint::Stroke;
use fastwave_backend::Timescale;
#[cfg(not(target_arch = "wasm32"))]
use fern::colors::ColoredLevelConfig;
use fzcmd::parse_command;
use log::error;
use log::info;
use log::trace;
use log::warn;
use message::Message;
use num::bigint::ToBigInt;
use num::BigInt;
use num::FromPrimitive;
use num::ToPrimitive;
use signal_filter::SignalFilterType;
use translation::all_translators;
use translation::spade::SpadeTranslator;
use translation::TranslatorList;
use viewport::Viewport;
use wasm_util::perform_work;
use wave_container::FieldRef;
use wave_container::SignalRef;
use wave_data::WaveData;
use wave_source::string_to_wavesource;
use wave_source::LoadProgress;
use wave_source::WaveSource;

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::mpsc::channel;
use std::sync::mpsc::{Receiver, Sender};

#[derive(clap::Parser, Default)]
struct Args {
    vcd_file: Option<String>,
    #[clap(long)]
    spade_state: Option<Utf8PathBuf>,
    #[clap(long)]
    spade_top: Option<String>,
    /// Path to a file containing 'commands' to run after a waveform has been loaded. The commands
    /// are the same as those used in the command line interface inside the program.
    /// Commands are separated by lines or ;. Empty lines are ignored. Line comments starting with
    /// `#` are supported
    /// NOTE: This feature is not permanent, it will be removed once a solid scripting system
    /// is implemented.
    #[clap(long, short)]
    command_file: Option<Utf8PathBuf>,
}

struct StartupParams {
    pub spade_state: Option<Utf8PathBuf>,
    pub spade_top: Option<String>,
    pub waves: Option<WaveSource>,
    pub startup_commands: Vec<String>,
}

impl StartupParams {
    #[allow(dead_code)] // NOTE: Only used in wasm version
    pub fn empty() -> Self {
        Self {
            spade_state: None,
            spade_top: None,
            waves: None,
            startup_commands: vec![],
        }
    }

    #[allow(dead_code)] // NOTE: Only used in wasm version
    pub fn vcd_from_url(url: Option<String>) -> Self {
        Self {
            spade_state: None,
            spade_top: None,
            waves: url.map(WaveSource::Url),
            startup_commands: vec![],
        }
    }

    #[allow(dead_code)] // NOTE: Only used in desktop version
    pub fn from_args(args: Args) -> Self {
        let startup_commands = if let Some(cmd_file) = args.command_file {
            std::fs::read_to_string(&cmd_file)
                .map_err(|e| error!("Failed to read commands from {cmd_file}. {e:#?}"))
                .ok()
                .map(|file_content| file_content.lines().map(|l| l.to_string()).collect())
                .unwrap_or_default()
        } else {
            vec![]
        };
        Self {
            spade_state: args.spade_state,
            spade_top: args.spade_top,
            waves: args.vcd_file.map(string_to_wavesource),
            startup_commands,
        }
    }

    pub fn with_startup_commands(mut self, startup_commands: Vec<String>) -> Self {
        self.startup_commands = startup_commands;
        self
    }
}

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    use logs::EGUI_LOGGER;

    color_eyre::install()?;

    let colors = ColoredLevelConfig::new()
        .error(fern::colors::Color::Red)
        .warn(fern::colors::Color::Yellow)
        .info(fern::colors::Color::Green)
        .debug(fern::colors::Color::Blue)
        .trace(fern::colors::Color::White);

    let egui_log_config = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .format(move |out, message, _record| out.finish(format_args!(" {}", message)))
        .chain(&EGUI_LOGGER as &(dyn log::Log + 'static));

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

    fern::Dispatch::new()
        .chain(stdout_config)
        .chain(egui_log_config)
        .apply()?;

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
            state.ui_scale = cc.egui_ctx.pixels_per_point();
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

    let mut state = State::new(StartupParams::vcd_from_url(url))?;

    wasm_bindgen_futures::spawn_local(async {
        eframe::WebRunner::new()
            .start(
                "the_canvas_id", // hardcode it
                web_options,
                Box::new(|cc| {
                    state.context = Some(cc.egui_ctx.clone());
                    state.ui_scale = cc.egui_ctx.pixels_per_point();
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
pub enum MoveDir {
    Up,
    Down,
}

pub enum ColorSpecifier {
    Index(usize),
    Name(String),
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

    /// The number of bytes loaded from the VCD file
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
    show_logs: bool,
    wanted_timescale: Timescale,
    gesture_start_location: Option<emath::Pos2>,
    show_url_entry: bool,
    signal_filter_focused: bool,
    signal_filter_type: SignalFilterType,
    rename_target: Option<usize>,

    ui_scale: f32,

    // List of unparsed commands to run at startup after the first wave has been loaded
    startup_commands: Vec<String>,

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
        let translators = all_translators();

        // Long running translators which we load in a thread
        {
            let sender = sender.clone();
            perform_work(move || {
                if let (Some(top), Some(state)) = (args.spade_top, args.spade_state) {
                    SpadeTranslator::load(&top, &state, sender);
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
            show_logs: false,
            wanted_timescale: Timescale::Unit,
            gesture_start_location: None,
            show_url_entry: false,
            rename_target: None,
            show_wave_source: true,
            signal_filter_focused: false,
            signal_filter_type: SignalFilterType::Fuzzy,
            ui_scale: 1.0,
            startup_commands: args.startup_commands,
            url: RefCell::new(String::new()),
            command_prompt_text: RefCell::new(String::new()),
            draw_data: RefCell::new(None),
            last_canvas_rect: RefCell::new(None),
            signal_filter: RefCell::new(String::new()),
            item_renaming_string: RefCell::new(String::new()),
        };

        match args.waves {
            Some(WaveSource::Url(url)) => result.load_vcd_from_url(url, false),
            Some(WaveSource::File(file)) => result.load_vcd_from_file(file, false).unwrap(),
            Some(WaveSource::DragAndDrop(_)) => {
                error!("Attempted to load from drag and drop at startup (how?)")
            }
            None => {}
        }

        Ok(result)
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::SetActiveScope(module) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                if waves.inner.has_module(&module) {
                    waves.active_module = Some(module)
                } else {
                    warn!("Setting active scope to {module} which does not exist")
                }
            }
            Message::AddSignal(sig) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.add_signal(&self.translators, &sig);
                    self.invalidate_draw_commands();
                }
            }
            Message::AddDivider(name) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.add_divider(name);
                }
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
                if let Some(waves) = self.waves.as_mut() {
                    waves.focused_item = None;
                };
            }
            Message::RenameItem(vidx) => {
                if let Some(waves) = self.waves.as_mut() {
                    self.rename_target = Some(vidx);
                    *self.item_renaming_string.borrow_mut() =
                        waves.displayed_items.get(vidx).unwrap().name();
                }
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
            Message::SetLogsVisible(visibility) => self.show_logs = visibility,
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
                if let Some(waves) = self.waves.as_mut() {
                    waves.remove_displayed_item(count, idx);
                    self.invalidate_draw_commands();
                }
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
                if let Some(waves) = self.waves.as_mut() {
                    waves.handle_canvas_scroll(delta);
                    self.invalidate_draw_commands();
                }
            }
            Message::CanvasZoom {
                delta,
                mouse_ptr_timestamp,
            } => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.handle_canvas_zoom(mouse_ptr_timestamp, delta as f64);
                    self.invalidate_draw_commands();
                }
            }
            Message::ZoomToFit => {
                if let Some(waves) = &mut self.waves {
                    waves.zoom_to_fit();
                    self.invalidate_draw_commands();
                }
            }
            Message::GoToEnd => {
                if let Some(waves) = &mut self.waves {
                    waves.go_to_end();
                    self.invalidate_draw_commands();
                }
            }
            Message::GoToStart => {
                if let Some(waves) = &mut self.waves {
                    waves.go_to_start();
                    self.invalidate_draw_commands();
                }
            }
            Message::SetTimeScale(timescale) => {
                self.invalidate_draw_commands();
                self.wanted_timescale = timescale;
            }
            Message::ZoomToRange { start, end } => {
                if let Some(waves) = &mut self.waves {
                    waves.viewport.curr_left = start;
                    waves.viewport.curr_right = end;
                    self.invalidate_draw_commands();
                }
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
                                    if disp.signal_ref == field.root {
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
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(idx) = vidx.or(waves.focused_item) {
                        waves.displayed_items[idx].set_color(color_name);
                    }
                };
            }
            Message::ItemNameChange(vidx, name) => {
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(idx) = vidx.or(waves.focused_item) {
                        waves.displayed_items[idx].set_name(name);
                    }
                };
            }
            Message::ItemBackgroundColorChange(vidx, color_name) => {
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(idx) = vidx.or(waves.focused_item) {
                        waves.displayed_items[idx].set_background_color(color_name)
                    }
                };
            }
            Message::ResetSignalFormat(idx) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.signal_format.remove(&idx);
                    self.invalidate_draw_commands();
                }
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
                self.run_startup_commands();
            }
            Message::BlacklistTranslator(idx, translator) => {
                self.blacklisted_translators.insert((idx, translator));
            }
            Message::Error(e) => {
                error!("{e:?}")
            }
            Message::TranslatorLoaded(t) => {
                info!("Translator {} loaded", t.name());
                self.translators.add_or_replace(t)
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
                    SurferConfig::new().with_context(|| "Failed to load config file")
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

                for translator in self.translators.all_translators() {
                    translator.reload(self.msg_sender.clone())
                }
            }
            Message::SetClockHighlightType(new_type) => {
                self.config.default_clock_highlight_type = new_type
            }
            Message::SetCursorPosition(idx) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.set_cursor_position(idx);
                };
            }
            Message::GoToCursorPosition(idx) => {
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(cursor) = waves.cursors.get(&idx) {
                        waves.go_to_time(&cursor.clone());
                        self.invalidate_draw_commands();
                    }
                };
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
                if let Some(waves) = self.waves.as_mut() {
                    waves.force_signal_name_type(name_type);
                };
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
                self.open_file_dialog(mode);
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
            Message::SetUiScale(scale) => {
                if let Some(ctx) = &mut self.context {
                    ctx.set_pixels_per_point(scale)
                }
                self.ui_scale = scale
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
            },
            ..Visuals::dark()
        }
    }

    pub fn run_startup_commands(&mut self) {
        let parsed = self
            .startup_commands
            .clone()
            .drain(0..(self.startup_commands.len()))
            // Add line numbers
            .enumerate()
            // Make the line numbers start at 1 as is tradition
            .map(|(no, line)| (no + 1, line))
            .map(|(no, line)| (no, line.trim().to_string()))
            // NOTE: Safe unwrap. Split will always return one element
            .map(|(no, line)| (no, line.split("#").next().unwrap().to_string()))
            .filter(|(_no, line)| !line.is_empty())
            .flat_map(|(no, line)| {
                line.split(";")
                    .map(|cmd| (no, cmd.to_string()))
                    .collect::<Vec<_>>()
            })
            .map(|(no, command)| {
                parse_command(&command, get_parser(&self)).map_err(|e| {
                    error!("Error on startup commands line {no}: {e:#?}");
                    e
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            // We reported the errors so we can just ignore everything if we had any errors
            .ok()
            .unwrap_or_default();

        for message in parsed {
            self.update(message)
        }
    }
}
