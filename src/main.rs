#[cfg(feature = "performance_plot")]
mod benchmark;
mod clock_highlighting;
mod command_prompt;
mod config;
#[cfg(not(target_arch = "wasm32"))]
mod cxxrtl;
#[cfg(not(target_arch = "wasm32"))]
mod cxxrtl_container;
mod displayed_item;
mod drawing_canvas;
mod help;
mod hierarchy;
mod keys;
mod logs;
mod marker;
mod menus;
mod message;
mod mousegestures;
mod overview;
mod state_util;
mod statusbar;
#[cfg(test)]
mod tests;
mod time;
mod toolbar;
mod translation;
mod util;
mod variable_name_filter;
mod variable_name_type;
mod variable_type;
mod view;
mod viewport;
#[cfg(target_arch = "wasm32")]
mod wasm_api;
mod wasm_util;
mod wave_container;
mod wave_data;
mod wave_source;
mod wellen;

#[cfg(feature = "performance_plot")]
use benchmark::Timing;
use camino::Utf8PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
use color_eyre::eyre::Context;
use color_eyre::Result;
use command_prompt::get_parser;
use config::SurferConfig;
use displayed_item::DisplayedItem;
use eframe::egui;
use eframe::egui::style::Selection;
use eframe::egui::style::WidgetVisuals;
use eframe::egui::style::Widgets;
use eframe::egui::FontDefinitions;
use eframe::egui::Visuals;
use eframe::emath;
use eframe::epaint::Rect;
use eframe::epaint::Rounding;
use eframe::epaint::Stroke;
#[cfg(not(target_arch = "wasm32"))]
use fern::colors::ColoredLevelConfig;
use fern::Dispatch;
use fzcmd::parse_command;
use lazy_static::lazy_static;
use log::error;
use log::info;
use log::trace;
use log::warn;
use logs::EGUI_LOGGER;
use message::Message;
use ron::ser::PrettyConfig;
use serde::Deserialize;
use serde::Serialize;
use time::TimeStringFormatting;
use time::TimeUnit;
use translation::all_translators;
#[cfg(feature = "spade")]
use translation::spade::SpadeTranslator;
use translation::TranslatorList;
use variable_name_filter::VariableNameFilterType;
use viewport::Viewport;
use wasm_util::perform_work;
use wasm_util::UrlArgs;
use wave_container::FieldRef;
use wave_container::VariableRef;
use wave_container::WaveContainer;
use wave_data::WaveData;
use wave_source::string_to_wavesource;
use wave_source::LoadOptions;
use wave_source::LoadProgress;
use wave_source::WaveFormat;
use wave_source::WaveSource;

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::sync::RwLock;

lazy_static! {
    pub static ref EGUI_CONTEXT: RwLock<Option<Arc<eframe::egui::Context>>> = RwLock::new(None);
}

#[derive(clap::Parser, Default)]
struct Args {
    /// Waveform file in VCD, FST, or GHW format.
    wave_file: Option<String>,
    #[clap(long)]
    spade_state: Option<Utf8PathBuf>,
    #[clap(long)]
    spade_top: Option<String>,
    /// Path to a file containing 'commands' to run after a waveform has been loaded.
    /// The commands are the same as those used in the command line interface inside the program.
    /// Commands are separated by lines or ;. Empty lines are ignored. Line comments starting with
    /// `#` are supported
    /// NOTE: This feature is not permanent, it will be removed once a solid scripting system
    /// is implemented.
    #[clap(long, short)]
    command_file: Option<Utf8PathBuf>,
    /// Alias for --command_file to mimic GTKWave and support VUnit
    #[clap(long)]
    script: Option<Utf8PathBuf>,

    #[clap(long, short)]
    state_file: Option<Utf8PathBuf>,
}

impl Args {
    pub fn command_file(&self) -> &Option<Utf8PathBuf> {
        if self.script.is_some() && self.command_file.is_some() {
            error!("At most one of --command_file and --script can be used");
            return &None;
        }
        if self.command_file.is_some() {
            &self.command_file
        } else {
            &self.script
        }
    }
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
    pub fn from_url(url: UrlArgs) -> Self {
        Self {
            spade_state: None,
            spade_top: None,
            waves: url.load_url.map(WaveSource::Url),
            startup_commands: url.startup_commands.map(|c| vec![c]).unwrap_or_default(),
        }
    }

    #[allow(dead_code)] // NOTE: Only used in desktop version
    pub fn from_args(args: Args) -> Self {
        let startup_commands = if let Some(cmd_file) = args.command_file() {
            std::fs::read_to_string(cmd_file)
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
            waves: args.wave_file.map(|s| string_to_wavesource(&s)),
            startup_commands,
        }
    }
}

fn setup_logging(platform_logger: Dispatch) -> Result<()> {
    let egui_log_config = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .level_for("surfer", log::LevelFilter::Trace)
        .format(move |out, message, _record| out.finish(format_args!(" {}", message)))
        .chain(&EGUI_LOGGER as &(dyn log::Log + 'static));

    fern::Dispatch::new()
        .chain(platform_logger)
        .chain(egui_log_config)
        .apply()?;
    Ok(())
}

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    let colors = ColoredLevelConfig::new()
        .error(fern::colors::Color::Red)
        .warn(fern::colors::Color::Yellow)
        .info(fern::colors::Color::Green)
        .debug(fern::colors::Color::Blue)
        .trace(fern::colors::Color::White);

    let stdout_config = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .level_for("surfer", log::LevelFilter::Trace)
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}] {}",
                colors.color(record.level()),
                message
            ))
        })
        .chain(std::io::stdout());
    setup_logging(stdout_config)?;

    color_eyre::install()?;

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
    let mut state = match &args.state_file {
        Some(file) => std::fs::read_to_string(file)
            .with_context(|| format!("Failed to read state from {file}"))
            .map(|content| {
                ron::from_str::<State>(&content)
                    .with_context(|| format!("Failed to decode state from {file}"))
            })
            .unwrap_or_else(|e| {
                error!("Failed to read state file. Opening fresh session\n{e:#?}");
                State::new()
            })?,
        None => State::new()?,
    }
    .with_params(StartupParams::from_args(args));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Surfer")
            .with_inner_size(egui::Vec2::new(
                state.config.layout.window_width as f32,
                state.config.layout.window_height as f32,
            )),
        ..Default::default()
    };

    eframe::run_native(
        "Surfer",
        options,
        Box::new(|cc| {
            let ctx_arc = Arc::new(cc.egui_ctx.clone());
            *EGUI_CONTEXT.write().unwrap() = Some(ctx_arc.clone());
            state.sys.context = Some(ctx_arc.clone());
            cc.egui_ctx.set_visuals(state.get_visuals());
            setup_custom_font(&cc.egui_ctx);
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

    let web_log_config = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .format(move |out, message, record| {
            out.finish(format_args!("[{}] {}", record.level(), message))
        })
        .chain(Box::new(eframe::WebLogger::new(log::LevelFilter::Debug)) as Box<dyn log::Log>);

    setup_logging(web_log_config)?;

    let web_options = eframe::WebOptions::default();

    let url = wasm_util::vcd_from_url();

    let mut state = State::new()?.with_params(StartupParams::from_url(url));

    wasm_bindgen_futures::spawn_local(async {
        eframe::WebRunner::new()
            .start(
                "the_canvas_id", // hardcode it
                web_options,
                Box::new(|cc| {
                    let ctx_arc = Arc::new(cc.egui_ctx.clone());
                    *EGUI_CONTEXT.write().unwrap() = Some(ctx_arc.clone());
                    state.sys.context = Some(ctx_arc.clone());
                    cc.egui_ctx.set_visuals(state.get_visuals());
                    setup_custom_font(&cc.egui_ctx);
                    Box::new(state)
                }),
            )
            .await
            .expect("failed to start eframe");
    });

    Ok(())
}

fn setup_custom_font(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    egui_remixicon::add_to_fonts(&mut fonts);

    ctx.set_fonts(fonts);
}

#[derive(Debug, Deserialize)]
pub enum MoveDir {
    Up,
    Down,
}

pub enum ColorSpecifier {
    Index(usize),
    Name(String),
}

struct CachedDrawData {
    pub draw_commands: HashMap<FieldRef, drawing_canvas::DrawingCommands>,
    pub clock_edges: Vec<f32>,
    pub ticks: Vec<(String, f32)>,
}

struct Channels {
    msg_sender: Sender<Message>,
    msg_receiver: Receiver<Message>,
}
impl Channels {
    fn new() -> Self {
        let (msg_sender, msg_receiver) = mpsc::channel();
        Self {
            msg_sender,
            msg_receiver,
        }
    }
}

pub struct SystemState {
    /// Which translator to use for each variable
    translators: TranslatorList,
    /// Channels for messages generated by other threads
    channels: Channels,

    /// The number of bytes loaded from the VCD file
    vcd_progress: Option<LoadProgress>,

    /// Buffer for the command input
    command_prompt: command_prompt::CommandPrompt,

    /// The context to egui, we need this to change the visual settings when the config is reloaded
    context: Option<Arc<eframe::egui::Context>>,

    // List of unparsed commands to run at startup after the first wave has been loaded
    startup_commands: Vec<String>,

    /// The draw commands for every variable currently selected
    // For performance reasons, these need caching so we have them in a RefCell for interior
    // mutability
    draw_data: RefCell<Vec<Option<CachedDrawData>>>,

    gesture_start_location: Option<emath::Pos2>,

    // Egui requires a place to store text field content between frames
    url: RefCell<String>,
    command_prompt_text: RefCell<String>,
    last_canvas_rect: RefCell<Option<Rect>>,
    variable_name_filter: RefCell<String>,
    item_renaming_string: RefCell<String>,

    // Benchmarking stuff
    /// Invalidate draw commands every frame to make performance comparison easier
    continuous_redraw: bool,
    #[cfg(feature = "performance_plot")]
    rendering_cpu_times: VecDeque<f32>,
    #[cfg(feature = "performance_plot")]
    timing: RefCell<Timing>,
}

impl SystemState {
    pub fn new() -> Self {
        let channels = Channels::new();

        // Basic translators that we can load quickly
        let translators = all_translators();

        Self {
            translators,
            channels,
            vcd_progress: None,
            command_prompt: command_prompt::CommandPrompt {
                visible: false,
                suggestions: vec![],
                selected: 0,
                previous_commands: vec![],
            },
            context: None,
            gesture_start_location: None,
            startup_commands: vec![],
            url: RefCell::new(String::new()),
            command_prompt_text: RefCell::new(String::new()),
            draw_data: RefCell::new(vec![None]),
            last_canvas_rect: RefCell::new(None),
            variable_name_filter: RefCell::new(String::new()),
            item_renaming_string: RefCell::new(String::new()),

            continuous_redraw: false,
            #[cfg(feature = "performance_plot")]
            rendering_cpu_times: VecDeque::new(),
            #[cfg(feature = "performance_plot")]
            timing: RefCell::new(Timing::new()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct State {
    #[serde(skip)]
    config: config::SurferConfig,

    /// Overrides for the config show_* fields. Defaults to `config.show_*` if not present
    show_hierarchy: Option<bool>,
    show_menu: Option<bool>,
    show_ticks: Option<bool>,
    show_toolbar: Option<bool>,
    show_tooltip: Option<bool>,
    show_overview: Option<bool>,
    show_statusbar: Option<bool>,
    align_names_right: Option<bool>,
    show_variable_indices: Option<bool>,

    waves: Option<WaveData>,

    previous_waves: Option<WaveData>,

    /// Count argument for movements
    count: Option<String>,

    // Vector of translators which have failed at the `translates` function for a variable.
    blacklisted_translators: HashSet<(VariableRef, String)>,

    show_about: bool,
    show_keys: bool,
    show_gestures: bool,
    show_quick_start: bool,
    show_license: bool,
    /// Hide the wave source. For now, this is only used in snapshot tests to avoid problems
    /// with absolute path diffs
    show_wave_source: bool,
    show_performance: bool,
    show_logs: bool,
    show_cursor_window: bool,
    wanted_timeunit: TimeUnit,
    time_string_format: Option<TimeStringFormatting>,
    show_url_entry: bool,
    variable_name_filter_focused: bool,
    variable_name_filter_type: VariableNameFilterType,
    variable_name_filter_case_insensitive: bool,
    rename_target: Option<usize>,

    /// UI Scale if set by the user
    ui_scale: Option<f32>,

    /// Internal state that does not persist between sessions and is not serialized
    #[serde(skip, default = "SystemState::new")]
    sys: SystemState,
}

impl State {
    fn new() -> Result<State> {
        Self::new_inner(false)
    }

    #[cfg(test)]
    fn new_default_config() -> Result<State> {
        Self::new_inner(true)
    }

    fn new_inner(force_default_config: bool) -> Result<State> {
        let config = config::SurferConfig::new(force_default_config)
            .with_context(|| "Failed to load config file")?;
        let result = State {
            sys: SystemState::new(),
            config,
            waves: None,
            previous_waves: None,
            count: None,
            blacklisted_translators: HashSet::new(),
            show_about: false,
            show_keys: false,
            show_gestures: false,
            show_performance: false,
            show_license: false,
            show_logs: false,
            show_cursor_window: false,
            wanted_timeunit: TimeUnit::None,
            time_string_format: None,
            show_url_entry: false,
            show_quick_start: false,
            rename_target: None,
            show_wave_source: true,
            variable_name_filter_focused: false,
            variable_name_filter_type: VariableNameFilterType::Fuzzy,
            variable_name_filter_case_insensitive: true,
            ui_scale: None,
            show_hierarchy: None,
            show_menu: None,
            show_ticks: None,
            show_toolbar: None,
            show_tooltip: None,
            show_overview: None,
            show_statusbar: None,
            align_names_right: None,
            show_variable_indices: None,
        };

        Ok(result)
    }

    fn with_params(mut self, args: StartupParams) -> Self {
        self.sys.startup_commands = args.startup_commands;

        self.previous_waves = self.waves;
        self.waves = None;

        // Long running translators which we load in a thread
        {
            #[cfg(feature = "spade")]
            let sender = self.sys.channels.msg_sender.clone();
            #[cfg(not(feature = "spade"))]
            let _ = self.sys.channels.msg_sender.clone();
            perform_work(move || {
                #[cfg(feature = "spade")]
                if let (Some(top), Some(state)) = (args.spade_top, args.spade_state) {
                    SpadeTranslator::load(&top, &state, sender);
                } else {
                    info!("spade-top and spade-state not set, not loading spade translator");
                }
                #[cfg(not(feature = "spade"))]
                if let (Some(_), Some(_)) = (args.spade_top, args.spade_state) {
                    info!("Surfer is not compiled with spade support, ignoring spade_top and spade_state");
                }
            });
        }

        match args.waves {
            Some(WaveSource::Url(url)) => self.load_vcd_from_url(url, LoadOptions::clean()),
            Some(WaveSource::File(file)) => self
                .load_wave_from_file(file, LoadOptions::clean())
                .unwrap(),
            Some(WaveSource::Data) => error!("Attempted to load data at startup"),
            #[cfg(not(target_arch = "wasm32"))]
            Some(WaveSource::CxxrtlTcp(url)) => {
                self.connect_to_cxxrtl(url, false);
            }
            Some(WaveSource::DragAndDrop(_)) => {
                error!("Attempted to load from drag and drop at startup (how?)")
            }
            None => {}
        }

        self
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::SetActiveScope(scope) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                if waves.inner.scope_exists(&scope) {
                    waves.active_scope = Some(scope)
                } else {
                    warn!("Setting active scope to {scope} which does not exist")
                }
            }
            Message::AddVariable(var) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.add_variable(&self.sys.translators, &var);
                    self.invalidate_draw_commands();
                }
            }
            Message::AddDivider(name, vidx) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.add_divider(name, vidx);
                }
            }
            Message::AddTimeLine(vidx) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.add_timeline(vidx);
                }
            }
            Message::AddScope(scope) => {
                let Some(waves) = self.waves.as_mut() else {
                    warn!("Adding scope without waves loaded");
                    return;
                };

                let variables = waves.inner.variables_in_scope(&scope);
                for variable in variables {
                    waves.add_variable(&self.sys.translators, &variable);
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
            Message::SetNameAlignRight(align_right) => {
                self.align_names_right = Some(align_right);
            }
            Message::FocusItem(idx) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                let visible_items_len = waves.displayed_items.len();
                if visible_items_len > 0 && idx < visible_items_len {
                    waves.focused_item = Some(idx);
                } else {
                    error!(
                        "Can not focus variable {idx} because only {visible_items_len} variables are visible.",
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
                    let idx = vidx.or(waves.focused_item);
                    if let Some(idx) = idx {
                        self.rename_target = Some(idx);
                        *self.sys.item_renaming_string.borrow_mut() = waves
                            .displayed_items_order
                            .get(idx)
                            .and_then(|id| waves.displayed_items.get(id))
                            .map(|item| item.name())
                            .unwrap_or_default();
                    }
                }
            }
            Message::MoveFocus(direction, count) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                let visible_items_len = waves.displayed_items.len();
                if visible_items_len > 0 {
                    self.count = None;
                    match direction {
                        MoveDir::Up => {
                            waves.focused_item = waves
                                .focused_item
                                .map_or(Some(visible_items_len - 1), |focused| {
                                    Some(focused - count.clamp(0, focused))
                                })
                        }
                        MoveDir::Down => {
                            waves.focused_item = waves.focused_item.map_or(
                                Some((count - 1).clamp(0, visible_items_len - 1)),
                                |focused| Some((focused + count).clamp(0, visible_items_len - 1)),
                            );
                        }
                    }
                }
            }
            Message::ScrollToItem(position) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.scroll_to_item(position);
                }
            }
            Message::SetScrollOffset(offset) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.scroll_offset = offset;
                }
            }
            Message::SetLogsVisible(visibility) => self.show_logs = visibility,
            Message::SetCursorWindowVisible(visibility) => self.show_cursor_window = visibility,
            Message::VerticalScroll(direction, count) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                let current_item = waves.get_top_item();
                match direction {
                    MoveDir::Down => {
                        waves.scroll_to_item(current_item + count);
                    }
                    MoveDir::Up => {
                        if current_item > count {
                            waves.scroll_to_item(current_item - count);
                        } else {
                            waves.scroll_to_item(0);
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
                    let visible_items_len = waves.displayed_items.len();
                    if visible_items_len > 0 {
                        match direction {
                            MoveDir::Up => {
                                for i in (idx
                                    .saturating_sub(count - 1)
                                    .clamp(1, visible_items_len - 1)
                                    ..=idx)
                                    .rev()
                                {
                                    waves.displayed_items_order.swap(i, i - 1);
                                    waves.focused_item = Some(i - 1);
                                }
                            }
                            MoveDir::Down => {
                                for i in idx..(idx + count).clamp(0, visible_items_len - 1) {
                                    waves.displayed_items_order.swap(i, i + 1);
                                    waves.focused_item = Some(i + 1);
                                }
                            }
                        }
                    }
                }
            }
            Message::CanvasScroll {
                delta,
                viewport_idx,
            } => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.viewports[viewport_idx]
                        .handle_canvas_scroll(delta.y as f64 + delta.x as f64);
                    self.invalidate_draw_commands();
                }
            }
            Message::CanvasZoom {
                delta,
                mouse_ptr,
                viewport_idx,
            } => {
                if let Some(waves) = self.waves.as_mut() {
                    let num_timestamps = waves.num_timestamps();
                    waves.viewports[viewport_idx].handle_canvas_zoom(
                        mouse_ptr,
                        delta as f64,
                        &num_timestamps,
                    );
                    self.invalidate_draw_commands();
                }
            }
            Message::ZoomToFit { viewport_idx } => {
                if let Some(waves) = &mut self.waves {
                    waves.viewports[viewport_idx].zoom_to_fit();
                    self.invalidate_draw_commands();
                }
            }
            Message::GoToEnd { viewport_idx } => {
                if let Some(waves) = &mut self.waves {
                    waves.viewports[viewport_idx].go_to_end();
                    self.invalidate_draw_commands();
                }
            }
            Message::GoToStart { viewport_idx } => {
                if let Some(waves) = &mut self.waves {
                    waves.viewports[viewport_idx].go_to_start();
                    self.invalidate_draw_commands();
                }
            }
            Message::GoToTime(time, viewport_idx) => {
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(time) = time {
                        let num_timestamps = waves.num_timestamps();
                        waves.viewports[viewport_idx].go_to_time(&time.clone(), &num_timestamps);
                        self.invalidate_draw_commands();
                    }
                };
            }
            Message::SetTimeUnit(timeunit) => {
                self.wanted_timeunit = timeunit;
                self.invalidate_draw_commands();
            }
            Message::SetTimeStringFormatting(format) => {
                self.time_string_format = format;
                self.invalidate_draw_commands();
            }
            Message::ZoomToRange {
                start,
                end,
                viewport_idx,
            } => {
                if let Some(waves) = &mut self.waves {
                    let num_timestamps = waves.num_timestamps();
                    waves.viewports[viewport_idx].zoom_to_range(&start, &end, &num_timestamps);
                    self.invalidate_draw_commands();
                }
            }
            Message::VariableFormatChange(field, format) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if self
                    .sys
                    .translators
                    .all_translator_names()
                    .contains(&&format)
                {
                    *waves.variable_format.entry(field.clone()).or_default() = format;

                    if field.field.is_empty() {
                        let Ok(meta) = waves
                            .inner
                            .variable_meta(&field.root)
                            .map_err(|e| warn!("{e:#?}"))
                        else {
                            return;
                        };
                        let translator = waves.variable_translator(&field, &self.sys.translators);
                        let new_info = translator.variable_info(&meta).unwrap();

                        for id in &waves.displayed_items_order {
                            waves
                                .displayed_items
                                .entry(*id)
                                .and_modify(|item| match item {
                                    DisplayedItem::Variable(disp) => {
                                        if disp.variable_ref == field.root {
                                            disp.info = new_info.clone();
                                        }
                                    }
                                    DisplayedItem::Marker(_) => {}
                                    DisplayedItem::Divider(_) => {}
                                    DisplayedItem::TimeLine(_) => {}
                                    DisplayedItem::Placeholder(_) => {}
                                });
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
                        waves.displayed_items_order.get(idx).map(|id| {
                            waves
                                .displayed_items
                                .entry(*id)
                                .and_modify(|item| item.set_color(color_name))
                        });
                    }
                };
            }
            Message::ItemNameChange(vidx, name) => {
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(idx) = vidx.or(waves.focused_item) {
                        waves.displayed_items_order.get(idx).map(|id| {
                            waves
                                .displayed_items
                                .entry(*id)
                                .and_modify(|item| item.set_name(name))
                        });
                    }
                };
            }
            Message::ItemBackgroundColorChange(vidx, color_name) => {
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(idx) = vidx.or(waves.focused_item) {
                        waves.displayed_items_order.get(idx).map(|id| {
                            waves
                                .displayed_items
                                .entry(*id)
                                .and_modify(|item| item.set_background_color(color_name))
                        });
                    }
                };
            }
            Message::MoveCursorToTransition {
                next,
                variable,
                skip_zero,
            } => {
                if let Some(waves) = &mut self.waves {
                    // if no cursor is set, move it to
                    // start of visible area transition for next transition
                    // end of visible area for previous transition
                    if waves.cursor.is_none() && waves.focused_item.is_some() {
                        let num_timestamps = waves.num_timestamps();
                        if next {
                            waves
                                .viewports
                                .first()
                                .map(|vp| waves.cursor = Some(vp.left_edge_time(&num_timestamps)));
                        } else {
                            waves
                                .viewports
                                .first()
                                .map(|vp| waves.cursor = Some(vp.right_edge_time(&num_timestamps)));
                        }
                    }
                    waves.set_cursor_at_transition(next, variable, skip_zero);
                    let moved = waves.go_to_cursor_if_not_in_view();
                    if moved {
                        self.invalidate_draw_commands();
                    }
                }
            }
            Message::ResetVariableFormat(idx) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.variable_format.remove(&idx);
                    self.invalidate_draw_commands();
                }
            }
            Message::CursorSet(new) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.cursor = Some(new)
                }
            }
            Message::RightCursorSet(new) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.right_cursor = new
                }
            }
            Message::LoadWaveformFile(filename, load_options) => {
                self.load_wave_from_file(filename, load_options).ok();
            }
            Message::LoadWaveformFileFromUrl(url, load_options) => {
                self.load_vcd_from_url(url, load_options);
            }
            Message::LoadWaveformFileFromData(data, load_options) => {
                self.load_vcd_from_data(data, load_options).ok();
            }
            #[cfg(not(target_arch = "wasm32"))]
            Message::ConnectToCxxrtl(url) => self.connect_to_cxxrtl(url, false),
            Message::FileDropped(dropped_file) => {
                self.load_vcd_from_dropped(dropped_file)
                    .map_err(|e| error!("{e:#?}"))
                    .ok();
            }
            Message::WavesLoaded(filename, format, new_waves, load_options) => {
                self.on_waves_loaded(filename, format, new_waves, load_options)
            }
            Message::BlacklistTranslator(idx, translator) => {
                self.blacklisted_translators.insert((idx, translator));
            }
            Message::Error(e) => {
                error!("{e:?}")
            }
            Message::TranslatorLoaded(t) => {
                info!("Translator {} loaded", t.name());
                self.sys.translators.add_or_replace(t)
            }
            Message::ToggleSidePanel => {
                let new = match self.show_hierarchy {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_hierarchy(),
                };
                self.show_hierarchy = Some(new)
            }
            Message::ToggleMenu => {
                let new = match self.show_menu {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_menu(),
                };
                self.show_menu = Some(new)
            }
            Message::ToggleToolbar => {
                let new = match self.show_toolbar {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_toolbar(),
                };
                self.show_toolbar = Some(new)
            }
            Message::ToggleStatusbar => {
                let new = match self.show_statusbar {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_statusbar(),
                };
                self.show_statusbar = Some(new)
            }
            Message::ToggleTickLines => {
                let new = match self.show_ticks {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_ticks(),
                };
                self.show_ticks = Some(new)
            }
            Message::ToggleVariableTooltip => {
                let new = match self.show_tooltip {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_tooltip(),
                };
                self.show_tooltip = Some(new)
            }
            Message::ToggleOverview => {
                let new = match self.show_overview {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_overview(),
                };
                self.show_overview = Some(new)
            }
            Message::ToggleIndices => {
                let new = match self.show_variable_indices {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_variable_indices(),
                };
                self.show_variable_indices = Some(new)
            }
            Message::ShowCommandPrompt(new_visibility) => {
                if !new_visibility {
                    *self.sys.command_prompt_text.borrow_mut() = "".to_string();
                    self.sys.command_prompt.suggestions = vec![];
                    self.sys.command_prompt.selected =
                        self.sys.command_prompt.previous_commands.len();
                }
                self.sys.command_prompt.visible = new_visibility;
            }
            Message::FileDownloaded(url, bytes, load_options) => {
                let size = bytes.len() as u64;
                self.load_vcd_from_bytes(
                    WaveSource::Url(url),
                    bytes.to_vec(),
                    Some(size),
                    load_options,
                )
            }
            Message::ReloadConfig => {
                // FIXME think about a structured way to collect errors
                if let Ok(config) =
                    SurferConfig::new(false).with_context(|| "Failed to load config file")
                {
                    self.config = config;
                    if let Some(ctx) = &self.sys.context.as_ref() {
                        ctx.set_visuals(self.get_visuals())
                    }
                }
            }
            Message::ReloadWaveform(keep_unavailable) => {
                let Some(waves) = &self.waves else { return };
                match &waves.source {
                    WaveSource::File(filename) => {
                        self.load_wave_from_file(
                            filename.clone(),
                            LoadOptions {
                                keep_variables: true,
                                keep_unavailable,
                                expect_format: Some(waves.format),
                            },
                        )
                        .ok();
                    }
                    WaveSource::Data => {} // can't reload
                    #[cfg(not(target_arch = "wasm32"))]
                    WaveSource::CxxrtlTcp(..) => {} // can't reload
                    WaveSource::DragAndDrop(filename) => {
                        filename.clone().and_then(|filename| {
                            self.load_wave_from_file(
                                filename,
                                LoadOptions {
                                    keep_variables: true,
                                    keep_unavailable,
                                    expect_format: None,
                                },
                            )
                            .ok()
                        });
                    }
                    WaveSource::Url(url) => {
                        self.load_vcd_from_url(
                            url.clone(),
                            LoadOptions {
                                keep_variables: true,
                                keep_unavailable,
                                expect_format: None,
                            },
                        );
                    }
                };

                for translator in self.sys.translators.all_translators() {
                    translator.reload(self.sys.channels.msg_sender.clone())
                }
            }
            Message::RemovePlaceholders => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.remove_placeholders()
                }
            }
            Message::SetClockHighlightType(new_type) => {
                self.config.default_clock_highlight_type = new_type
            }
            Message::SetMarker { id, time } => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.set_marker_position(id, &time)
                };
            }
            Message::MoveMarkerToCursor(idx) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.move_marker_to_cursor(idx);
                };
            }
            Message::GoToMarkerPosition(idx, viewport_idx) => {
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(cursor) = waves.markers.get(&idx) {
                        let num_timestamps = waves.num_timestamps();
                        waves.viewports[viewport_idx].go_to_time(cursor, &num_timestamps);
                        self.invalidate_draw_commands();
                    }
                };
            }
            Message::ChangeVariableNameType(vidx, name_type) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                // checks if vidx is Some then use that, else try focused variable
                if let Some(idx) = vidx.or(waves.focused_item) {
                    if waves.displayed_items.len() > idx {
                        let id = waves.displayed_items_order[idx];
                        let mut recompute_names = false;
                        waves.displayed_items.entry(id).and_modify(|item| {
                            if let DisplayedItem::Variable(variable) = item {
                                variable.display_name_type = name_type;
                                recompute_names = true;
                            }
                        });
                        if recompute_names {
                            waves.compute_variable_display_names();
                        }
                    }
                }
            }
            Message::ForceVariableNameTypes(name_type) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.force_variable_name_type(name_type);
                };
            }
            Message::CommandPromptClear => {
                *self.sys.command_prompt_text.borrow_mut() = "".to_string();
                self.sys.command_prompt.suggestions = vec![];
                // self.sys.command_prompt.selected = self.sys.command_prompt.previous_commands.len();
                self.sys.command_prompt.selected =
                    if self.sys.command_prompt_text.borrow().is_empty() {
                        self.sys.command_prompt.previous_commands.len().clamp(0, 3)
                    } else {
                        0
                    };
            }
            Message::CommandPromptUpdate { suggestions } => {
                self.sys.command_prompt.suggestions = suggestions;
                self.sys.command_prompt.selected =
                    if self.sys.command_prompt_text.borrow().is_empty() {
                        self.sys.command_prompt.previous_commands.len().clamp(0, 3)
                    } else {
                        0
                    };
            }
            Message::CommandPromptPushPrevious(cmd) => {
                let len = cmd.len();
                self.sys
                    .command_prompt
                    .previous_commands
                    .insert(0, (cmd, vec![false; len]));
            }
            Message::OpenFileDialog(mode) => {
                self.open_file_dialog(mode);
            }
            Message::OpenSaveStateDialog => {
                self.open_state_save_dialog();
            }
            Message::SetAboutVisible(s) => self.show_about = s,
            Message::SetKeyHelpVisible(s) => self.show_keys = s,
            Message::SetGestureHelpVisible(s) => self.show_gestures = s,
            Message::SetUrlEntryVisible(s) => self.show_url_entry = s,
            Message::SetLicenseVisible(s) => self.show_license = s,
            Message::SetQuickStartVisible(s) => self.show_quick_start = s,
            Message::SetRenameItemVisible(_) => self.rename_target = None,
            Message::SetPerformanceVisible(s) => {
                if !s {
                    self.sys.continuous_redraw = false
                }
                self.show_performance = s
            }
            Message::SetContinuousRedraw(s) => self.sys.continuous_redraw = s,
            Message::SetDragStart(pos) => self.sys.gesture_start_location = pos,
            Message::SetFilterFocused(s) => self.variable_name_filter_focused = s,
            Message::SetVariableNameFilterType(variable_name_filter_type) => {
                self.variable_name_filter_type = variable_name_filter_type
            }
            Message::SetVariableNameFilterCaseInsensitive(s) => {
                self.variable_name_filter_case_insensitive = s
            }
            Message::SetUiScale(scale) => {
                if let Some(ctx) = &mut self.sys.context.as_ref() {
                    ctx.set_pixels_per_point(scale)
                }
                self.ui_scale = Some(scale)
            }
            Message::SaveState(filename) => {
                self.save_state(&filename);
            }
            Message::SelectPrevCommand => {
                self.sys.command_prompt.selected =
                    self.sys.command_prompt.selected.saturating_sub(1);
            }
            Message::SelectNextCommand => {
                self.sys.command_prompt.selected = std::cmp::min(
                    self.sys.command_prompt.selected + 1,
                    self.sys.command_prompt.suggestions.len().saturating_sub(1),
                );
            }
            Message::SetHierarchyStyle(style) => self.config.layout.hierarchy_style = style,
            Message::SetArrowKeyBindings(bindings) => {
                self.config.behavior.arrow_key_bindings = bindings
            }
            Message::InvalidateDrawCommands => self.invalidate_draw_commands(),
            Message::UnpauseSimulation => {
                if let Some(waves) = &self.waves {
                    waves.inner.unpause_simulation()
                }
            }
            Message::PauseSimulation => {
                if let Some(waves) = &self.waves {
                    waves.inner.pause_simulation()
                }
            }
            Message::Batch(messages) => {
                for message in messages {
                    self.update(message)
                }
            }
            Message::VariableValueToClipbord(vidx) => {
                if let Some(waves) = &self.waves {
                    if let Some(vidx) = vidx.or(waves.focused_item) {
                        if let Some(DisplayedItem::Variable(displayed_variable)) = waves
                            .displayed_items_order
                            .get(vidx)
                            .and_then(|id| waves.displayed_items.get(id))
                        {
                            let path =
                                FieldRef::without_fields(displayed_variable.variable_ref.clone());
                            let variable_value = self.get_variable_value(
                                waves,
                                &path,
                                &waves.cursor.as_ref().and_then(|u| u.to_biguint()),
                            );
                            if let Some(variable_value) = variable_value {
                                if let Some(ctx) = &self.sys.context {
                                    ctx.output_mut(|o| o.copied_text = variable_value);
                                }
                            }
                        }
                    }
                }
            }
            Message::Exit | Message::ToggleFullscreen => {} // Handled in eframe::update
            Message::AddViewport => {
                if let Some(waves) = &mut self.waves {
                    let viewport = Viewport::new();
                    waves.viewports.push(viewport);
                    self.sys.draw_data.borrow_mut().push(None);
                }
            }
            Message::RemoveViewport => {
                if let Some(waves) = &mut self.waves {
                    if waves.viewports.len() > 1 {
                        waves.viewports.pop();
                        self.sys.draw_data.borrow_mut().pop();
                    }
                }
            }
        }
    }

    fn on_waves_loaded(
        &mut self,
        filename: WaveSource,
        format: WaveFormat,
        new_waves: Box<WaveContainer>,
        load_options: LoadOptions,
    ) {
        info!("{format} file loaded");
        let viewport = Viewport::new();
        let viewports = [viewport].to_vec();

        let new_wave = if load_options.keep_variables && self.waves.is_some() {
            self.waves.take().unwrap().update_with(
                new_waves,
                filename,
                format,
                &self.sys.translators,
                load_options.keep_unavailable,
            )
        } else if let Some(old) = self.previous_waves.take() {
            old.update_with(
                new_waves,
                filename,
                format,
                &self.sys.translators,
                load_options.keep_unavailable,
            )
        } else {
            WaveData {
                inner: *new_waves,
                source: filename,
                format,
                active_scope: None,
                displayed_items_order: vec![],
                displayed_items: HashMap::new(),
                viewports,
                variable_format: HashMap::new(),
                cursor: None,
                right_cursor: None,
                markers: HashMap::new(),
                focused_item: None,
                default_variable_name_type: self.config.default_variable_name_type,
                scroll_offset: 0.,
                drawing_infos: vec![],
                top_item_draw_offset: 0.,
                total_height: 0.,
                display_item_ref_counter: 0,
            }
        };
        self.invalidate_draw_commands();

        // Set time unit to the file time unit before consuming new_wave
        self.wanted_timeunit = new_wave.inner.metadata().timescale.unit;
        self.waves = Some(new_wave);
        self.sys.vcd_progress = None;
        info!("Done setting up VCD file");
        self.run_startup_commands();
    }

    fn handle_async_messages(&mut self) {
        let mut msgs = vec![];
        loop {
            match self.sys.channels.msg_receiver.try_recv() {
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
        let startup_commands = &self.sys.startup_commands;
        trace!("Parsing startup commands {:?}", startup_commands);
        let parsed = startup_commands
            .clone()
            .drain(0..(startup_commands.len()))
            // Add line numbers
            .enumerate()
            // Make the line numbers start at 1 as is tradition
            .map(|(no, line)| (no + 1, line))
            .map(|(no, line)| (no, line.trim().to_string()))
            // NOTE: Safe unwrap. Split will always return one element
            .map(|(no, line)| (no, line.split('#').next().unwrap().to_string()))
            .filter(|(_no, line)| !line.is_empty())
            .flat_map(|(no, line)| {
                line.split(';')
                    .map(|cmd| (no, cmd.to_string()))
                    .collect::<Vec<_>>()
            })
            .filter_map(|(no, command)| {
                parse_command(&command, get_parser(self))
                    .map_err(|e| {
                        error!("Error on startup commands line {no}: {e:#?}");
                        e
                    })
                    .map(|message| (message, command))
                    .ok()
            })
            .collect::<Vec<_>>();

        for (message, message_string) in parsed {
            info!("Applying message {message_string}");
            self.update(message)
        }
    }

    fn encode_state(&self) -> Option<String> {
        let opt = ron::Options::default();
        opt.to_string_pretty(self, PrettyConfig::default())
            .context("Failed to encode state")
            .map_err(|e| error!("Failed to encode state. {e:#?}"))
            .ok()
    }

    fn save_state(&self, filename: &Path) {
        let filename_str = filename.to_string_lossy();
        self.encode_state().and_then(|ser| {
            std::fs::write(filename, ser)
                .context(format!("Failed to write state to {filename_str}"))
                .map_err(|e| error!("Failed to write state. {e:#?}"))
                .ok()
        });
    }
}
