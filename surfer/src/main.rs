#![deny(unused_crate_dependencies)]

#[cfg(feature = "performance_plot")]
mod benchmark;
mod clock_highlighting;
mod command_prompt;
mod config;
#[cfg(not(target_arch = "wasm32"))]
mod cxxrtl;
#[cfg(not(target_arch = "wasm32"))]
mod cxxrtl_container;
mod data_container;
mod dialog;
mod displayed_item;
mod drawing_canvas;
mod file_watcher;
mod graphics;
mod help;
mod hierarchy;
mod keys;
mod logs;
mod marker;
mod menus;
mod message;
mod mousegestures;
mod overview;
mod remote;
mod state_util;
mod statusbar;
#[cfg(test)]
mod tests;
mod time;
mod toolbar;
mod transaction_container;
mod translation;
mod util;
mod variable_direction;
mod variable_name_filter;
mod variable_name_type;
mod variable_type;
mod view;
mod viewport;
mod wasm_api;
#[cfg(target_arch = "wasm32")]
mod wasm_panic;
mod wasm_util;
mod wave_container;
mod wave_data;
mod wave_source;
mod wellen;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::mem;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};

use camino::Utf8PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
use color_eyre::eyre::Context;
use color_eyre::Result;
use derive_more::Display;
use eframe::App;
use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{FontData, FontDefinitions, FontFamily, Visuals};
#[cfg(not(target_arch = "wasm32"))]
use emath::Vec2;
use emath::{Pos2, Rect};
use epaint::{Rounding, Stroke};
use ftr_parser::types::Transaction;
use fzcmd::parse_command;
use itertools::Itertools;
use lazy_static::lazy_static;
use log::{error, info, trace, warn};
use num::BigInt;
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use surfer_translation_types::Translator;
use time::{TimeStringFormatting, TimeUnit};

#[cfg(feature = "performance_plot")]
use crate::benchmark::Timing;
use crate::command_prompt::get_parser;
use crate::config::{SurferConfig, SurferTheme};
use crate::data_container::DataContainer;
use crate::data_container::DataContainer::Transactions;
use crate::dialog::ReloadWaveformDialog;
use crate::displayed_item::{
    DisplayedFieldRef, DisplayedItem, DisplayedItemIndex, DisplayedItemRef, FieldFormat,
};
use crate::drawing_canvas::TxDrawingCommands;
use crate::file_watcher::FileWatcher;
use crate::message::{HeaderResult, Message};
use crate::transaction_container::{
    StreamScopeRef, TransactionContainer, TransactionRef, TransactionStreamRef,
};
#[cfg(feature = "spade")]
use crate::translation::spade::SpadeTranslator;
use crate::translation::{all_translators, AnyTranslator, TranslatorList};
use crate::variable_name_filter::VariableNameFilterType;
use crate::viewport::Viewport;
use crate::wasm_util::{perform_work, UrlArgs};
use crate::wave_container::{ScopeRef, ScopeRefExt, VariableRef, WaveContainer};
use crate::wave_data::{ScopeType, WaveData};
use crate::wave_source::{string_to_wavesource, LoadOptions, LoadProgress, WaveFormat, WaveSource};
use crate::wellen::convert_format;

lazy_static! {
    pub static ref EGUI_CONTEXT: RwLock<Option<Arc<egui::Context>>> = RwLock::new(None);
}

#[derive(clap::Parser, Default)]
#[command(version, about)]
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
    #[clap(long, short, verbatim_doc_comment)]
    command_file: Option<Utf8PathBuf>,
    /// Alias for --command_file to mimic GTKWave and support VUnit
    #[clap(long)]
    script: Option<Utf8PathBuf>,

    #[clap(long, short)]
    state_file: Option<Utf8PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
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

#[derive(clap::Subcommand)]
enum Commands {
    #[cfg(not(target_arch = "wasm32"))]
    /// starts surfer in headless mode so that a user can connect to it
    Server {
        /// port on which server will listen
        #[clap(long)]
        port: Option<u16>,
        /// token used by the client to authenticate to the server
        #[clap(long)]
        token: Option<String>,
        /// waveform file that we want to serve
        #[arg(long)]
        file: String,
    },
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
                .map(|file_content| {
                    file_content
                        .lines()
                        .map(std::string::ToString::to_string)
                        .collect()
                })
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

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    logs::start_logging()?;

    // https://tokio.rs/tokio/topics/bridging
    // We want to run the gui in the main thread, but some long running tasks like
    // loading VCDs should be done asynchronously. We can't just use std::thread to
    // do that due to wasm support, so we'll start a tokio runtime
    let runtime = tokio::runtime::Builder::new_current_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();

    // parse arguments
    let args = Args::parse();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(Commands::Server { port, token, file }) = args.command {
        let default_port = 8911; // FIXME: make this more configurable
        let res = runtime.block_on(surver::server_main(
            port.unwrap_or(default_port),
            token,
            file,
            None,
        ));
        return res;
    }

    let _enter = runtime.enter();

    std::thread::spawn(move || {
        runtime.block_on(async {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            }
        });
    });

    let state_file = args.state_file.clone();
    let startup_params = StartupParams::from_args(args);
    let waves = startup_params.waves.clone();

    let mut state = match &state_file {
        Some(file) => std::fs::read_to_string(file)
            .with_context(|| format!("Failed to read state from {file}"))
            .and_then(|content| {
                ron::from_str::<State>(&content)
                    .with_context(|| format!("Failed to decode state from {file}"))
            })
            .map(|mut s| {
                s.state_file = Some(file.into());
                s
            })
            .or_else(|e| {
                error!("Failed to read state file. Opening fresh session\n{e:#?}");
                State::new()
            })?,
        None => State::new()?,
    }
    .with_params(startup_params);

    // install a file watcher that emits a `SuggestReloadWaveform` message
    // whenever the user-provided file changes.
    let _watcher = match waves {
        Some(WaveSource::File(path)) => {
            let sender = state.sys.channels.msg_sender.clone();
            FileWatcher::new(&path, move || {
                match sender.send(Message::SuggestReloadWaveform) {
                    Ok(_) => {}
                    Err(err) => {
                        error!("Message ReloadWaveform did not send:\n{err}")
                    }
                }
            })
            .inspect_err(|err| error!("Cannot set up the file watcher:\n{err}"))
            .ok()
        }
        _ => None,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Surfer")
            .with_inner_size(Vec2::new(
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
            Ok(Box::new(state))
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

    logs::setup_logging(web_log_config)?;

    wasm_panic::set_once();

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
                    Ok(Box::new(state))
                }),
            )
            .await
            .expect("failed to start eframe");
    });

    Ok(())
}

fn setup_custom_font(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "remix_icons".to_owned(),
        FontData::from_static(egui_remixicon::FONT),
    );

    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .push("remix_icons".to_owned());

    ctx.set_fonts(fonts);
}

#[derive(Debug, Deserialize, Display)]
pub enum MoveDir {
    #[display("up")]
    Up,

    #[display("down")]
    Down,
}

pub enum ColorSpecifier {
    Index(usize),
    Name(String),
}

enum CachedDrawData {
    WaveDrawData(CachedWaveDrawData),
    TransactionDrawData(CachedTransactionDrawData),
}

struct CachedWaveDrawData {
    pub draw_commands: HashMap<DisplayedFieldRef, drawing_canvas::DrawingCommands>,
    pub clock_edges: Vec<f32>,
    pub ticks: Vec<(String, f32)>,
}

struct CachedTransactionDrawData {
    pub draw_commands: HashMap<TransactionRef, TxDrawingCommands>,
    pub stream_to_displayed_txs: HashMap<TransactionStreamRef, Vec<TransactionRef>>,
    pub inc_relation_tx_ids: Vec<TransactionRef>,
    pub out_relation_tx_ids: Vec<TransactionRef>,
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

/// Stores the current canvas state to enable undo/redo operations
struct CanvasState {
    message: String,
    focused_item: Option<DisplayedItemIndex>,
    focused_transaction: (Option<TransactionRef>, Option<Transaction>),
    selected_items: HashSet<DisplayedItemRef>,
    displayed_item_order: Vec<DisplayedItemRef>,
    displayed_items: HashMap<DisplayedItemRef, DisplayedItem>,
    markers: HashMap<u8, BigInt>,
}

pub struct SystemState {
    /// Which translator to use for each variable
    translators: TranslatorList,
    /// Channels for messages generated by other threads
    channels: Channels,

    /// Tracks progress of file/variable loading operations.
    progress_tracker: Option<LoadProgress>,

    /// Buffer for the command input
    command_prompt: command_prompt::CommandPrompt,

    /// The context to egui, we need this to change the visual settings when the config is reloaded
    context: Option<Arc<egui::Context>>,

    /// List of batch commands which will executed as soon as possible
    batch_commands: VecDeque<Message>,
    batch_commands_completed: bool,

    /// The draw commands for every variable currently selected
    // For performance reasons, these need caching so we have them in a RefCell for interior
    // mutability
    draw_data: RefCell<Vec<Option<CachedDrawData>>>,

    gesture_start_location: Option<Pos2>,

    // Egui requires a place to store text field content between frames
    url: RefCell<String>,
    command_prompt_text: RefCell<String>,
    last_canvas_rect: RefCell<Option<Rect>>,
    variable_name_filter: RefCell<String>,
    item_renaming_string: RefCell<String>,

    /// These items should be expanded into subfields in the next frame. Cleared after each
    /// frame
    items_to_expand: RefCell<Vec<(DisplayedItemRef, usize)>>,
    /// Character to add to the command prompt if it is visible. This is only needed for
    /// presentations at them moment.
    char_to_add_to_prompt: RefCell<Option<char>>,

    // Benchmarking stuff
    /// Invalidate draw commands every frame to make performance comparison easier
    continuous_redraw: bool,
    #[cfg(feature = "performance_plot")]
    rendering_cpu_times: VecDeque<f32>,
    #[cfg(feature = "performance_plot")]
    timing: RefCell<Timing>,

    // Undo and Redo stacks
    undo_stack: Vec<CanvasState>,
    redo_stack: Vec<CanvasState>,
}

impl Default for SystemState {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemState {
    pub fn new() -> Self {
        let channels = Channels::new();

        // Basic translators that we can load quickly
        let translators = all_translators();

        Self {
            translators,
            channels,
            progress_tracker: None,
            command_prompt: command_prompt::CommandPrompt {
                visible: false,
                suggestions: vec![],
                selected: 0,
                new_selection: None,
                new_cursor_pos: None,
                previous_commands: vec![],
            },
            context: None,
            gesture_start_location: None,
            batch_commands: VecDeque::new(),
            batch_commands_completed: false,
            url: RefCell::new(String::new()),
            command_prompt_text: RefCell::new(String::new()),
            draw_data: RefCell::new(vec![None]),
            last_canvas_rect: RefCell::new(None),
            variable_name_filter: RefCell::new(String::new()),
            item_renaming_string: RefCell::new(String::new()),

            items_to_expand: RefCell::new(vec![]),
            char_to_add_to_prompt: RefCell::new(None),

            continuous_redraw: false,
            #[cfg(feature = "performance_plot")]
            rendering_cpu_times: VecDeque::new(),
            #[cfg(feature = "performance_plot")]
            timing: RefCell::new(Timing::new()),
            undo_stack: vec![],
            redo_stack: vec![],
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
    show_variable_direction: Option<bool>,
    show_empty_scopes: Option<bool>,
    show_parameters_in_scopes: Option<bool>,

    waves: Option<WaveData>,
    drag_started: bool,
    drag_source_idx: Option<DisplayedItemIndex>,
    drag_target_idx: Option<DisplayedItemIndex>,

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
    show_performance: bool,
    show_logs: bool,
    show_cursor_window: bool,
    wanted_timeunit: TimeUnit,
    time_string_format: Option<TimeStringFormatting>,
    show_url_entry: bool,
    /// Show a confirmation dialog asking the user for confirmation
    /// that surfer should reload changed files from disk.
    #[serde(skip, default)]
    show_reload_suggestion: Option<ReloadWaveformDialog>,
    variable_name_filter_focused: bool,
    variable_name_filter_type: VariableNameFilterType,
    variable_name_filter_case_insensitive: bool,
    rename_target: Option<DisplayedItemIndex>,
    //Sidepanel width
    sidepanel_width: Option<f32>,
    /// UI zoom factor if set by the user
    ui_zoom_factor: Option<f32>,

    // Path of last saved-to state file
    // Do not serialize as this causes a few issues and doesn't help:
    // - We need to set it on load of a state anyways since the file could have been renamed
    // - Bad interoperatility story between native and wasm builds
    // - Sequencing issue in serialization, due to us having to run that async
    #[serde(skip)]
    state_file: Option<PathBuf>,

    /// Internal state that does not persist between sessions and is not serialized
    #[serde(skip, default = "SystemState::new")]
    sys: SystemState,
}

// Impl needed since for loading we need to put State into a Message
// Snip out the actual contents to not completely spam the terminal
impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "State {{ <snipped> }}")
    }
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
            show_reload_suggestion: None,
            rename_target: None,
            variable_name_filter_focused: false,
            variable_name_filter_type: VariableNameFilterType::Fuzzy,
            variable_name_filter_case_insensitive: true,
            ui_zoom_factor: None,
            show_hierarchy: None,
            show_menu: None,
            show_ticks: None,
            show_toolbar: None,
            show_tooltip: None,
            show_overview: None,
            show_statusbar: None,
            show_variable_direction: None,
            align_names_right: None,
            show_variable_indices: None,
            show_empty_scopes: None,
            show_parameters_in_scopes: None,
            drag_started: false,
            drag_source_idx: None,
            drag_target_idx: None,
            state_file: None,
            sidepanel_width: None,
        };

        Ok(result)
    }

    fn with_params(mut self, args: StartupParams) -> Self {
        self.previous_waves = self.waves;
        self.waves = None;

        // Long running translators which we load in a thread
        {
            #[cfg(feature = "spade")]
            let sender = self.sys.channels.msg_sender.clone();
            #[cfg(not(feature = "spade"))]
            let _ = self.sys.channels.msg_sender.clone();
            let waves = args.waves.clone();
            perform_work(move || {
                #[cfg(feature = "spade")]
                SpadeTranslator::load(&waves, &args.spade_top, &args.spade_state, sender);
                #[cfg(not(feature = "spade"))]
                if let (Some(_), Some(_)) = (args.spade_top, args.spade_state) {
                    info!("Surfer is not compiled with spade support, ignoring spade_top and spade_state");
                }
            });
        }

        // we turn the waveform argument and any startup command file into batch commands
        self.sys.batch_commands = VecDeque::new();

        match args.waves {
            Some(WaveSource::Url(url)) => {
                self.add_startup_message(Message::LoadWaveformFileFromUrl(
                    url,
                    LoadOptions::clean(),
                ));
            }
            Some(WaveSource::File(file)) => {
                self.add_startup_message(Message::LoadFile(file, LoadOptions::clean()));
            }
            Some(WaveSource::Data) => error!("Attempted to load data at startup"),
            #[cfg(not(target_arch = "wasm32"))]
            Some(WaveSource::CxxrtlTcp(url)) => {
                self.add_startup_message(Message::ConnectToCxxrtl(url));
            }
            Some(WaveSource::DragAndDrop(_)) => {
                error!("Attempted to load from drag and drop at startup (how?)");
            }
            None => {}
        }

        self.add_startup_commands(args.startup_commands);

        self
    }

    pub fn add_startup_commands<I: IntoIterator<Item = String>>(&mut self, commands: I) {
        let parsed = self.parse_startup_commands(commands);
        for msg in parsed {
            self.sys.batch_commands.push_back(msg);
            self.sys.batch_commands_completed = false;
        }
    }

    pub fn add_startup_messages<I: IntoIterator<Item = Message>>(&mut self, messages: I) {
        for msg in messages {
            self.sys.batch_commands.push_back(msg);
            self.sys.batch_commands_completed = false;
        }
    }

    pub fn add_startup_message(&mut self, msg: Message) {
        self.add_startup_messages([msg]);
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::SetActiveScope(scope) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                let scope = if let ScopeType::StreamScope(StreamScopeRef::Empty(name)) = scope {
                    ScopeType::StreamScope(StreamScopeRef::new_stream_from_name(
                        waves.inner.as_transactions().unwrap(),
                        name,
                    ))
                } else {
                    scope
                };

                if waves.inner.scope_exists(&scope) {
                    waves.active_scope = Some(scope);
                } else {
                    warn!("Setting active scope to {scope} which does not exist");
                }
            }
            Message::AddVariables(vars) => {
                if !vars.is_empty() {
                    let undo_msg = if vars.len() == 1 {
                        format!("Add variable {}", vars[0].name)
                    } else {
                        format!("Add {} variables", vars.len())
                    };
                    self.save_current_canvas(undo_msg);
                    if let Some(waves) = self.waves.as_mut() {
                        if let Some(cmd) = waves.add_variables(&self.sys.translators, vars) {
                            self.load_variables(cmd);
                        }
                        self.invalidate_draw_commands();
                    } else {
                        error!("Could not load signals, no waveform loaded");
                    }
                }
            }
            Message::AddDivider(name, vidx) => {
                self.save_current_canvas("Add divider".into());
                if let Some(waves) = self.waves.as_mut() {
                    waves.add_divider(name, vidx);
                }
            }
            Message::AddTimeLine(vidx) => {
                self.save_current_canvas("Add timeline".into());
                if let Some(waves) = self.waves.as_mut() {
                    waves.add_timeline(vidx);
                }
            }
            Message::AddScope(scope, recursive) => {
                self.save_current_canvas(format!("Add scope {}", scope.name()));
                self.add_scope(scope, recursive);
            }
            Message::AddCount(digit) => {
                if let Some(count) = &mut self.count {
                    count.push(digit);
                } else {
                    self.count = Some(digit.to_string());
                }
            }
            Message::AddStreamOrGenerator(s) => {
                let undo_msg = if let Some(gen_id) = s.gen_id {
                    format!("Add generator(id: {})", gen_id)
                } else {
                    format!("Add stream(id: {})", s.stream_id)
                };
                self.save_current_canvas(undo_msg);

                if let Some(waves) = self.waves.as_mut() {
                    if s.gen_id.is_some() {
                        waves.add_generator(s);
                    } else {
                        waves.add_stream(s);
                    }
                    self.invalidate_draw_commands();
                }
            }
            Message::AddStreamOrGeneratorFromName(scope, name) => {
                self.save_current_canvas(format!(
                    "Add Stream/Generator from name: {}",
                    name.clone()
                ));
                if let Some(waves) = self.waves.as_mut() {
                    let Some(inner) = waves.inner.as_transactions() else {
                        return;
                    };
                    if let Some(scope) = scope {
                        match scope {
                            StreamScopeRef::Root => {
                                let (stream_id, name) = inner
                                    .get_stream_from_name(name)
                                    .map(|s| (s.id, s.name.clone()))
                                    .unwrap();

                                waves.add_stream(TransactionStreamRef::new_stream(stream_id, name));
                            }
                            StreamScopeRef::Stream(stream) => {
                                let (stream_id, id, name) = inner
                                    .get_generator_from_name(Some(stream.stream_id), name)
                                    .map(|gen| (gen.stream_id, gen.id, gen.name.clone()))
                                    .unwrap();

                                waves.add_generator(TransactionStreamRef::new_gen(
                                    stream_id, id, name,
                                ));
                            }
                            StreamScopeRef::Empty(_) => {}
                        }
                    } else {
                        let (stream_id, id, name) = inner
                            .get_generator_from_name(None, name)
                            .map(|gen| (gen.stream_id, gen.id, gen.name.clone()))
                            .unwrap();

                        waves.add_generator(TransactionStreamRef::new_gen(stream_id, id, name));
                    }
                    self.invalidate_draw_commands();
                }
            }
            Message::AddAllFromStreamScope(scope_name) => {
                self.save_current_canvas(format!("Add all from scope {}", scope_name.clone()));
                if let Some(waves) = self.waves.as_mut() {
                    if scope_name == "tr" {
                        waves.add_all_streams();
                    } else {
                        let Some(inner) = waves.inner.as_transactions() else {
                            return;
                        };
                        if let Some(stream) = inner.get_stream_from_name(scope_name) {
                            let gens = stream
                                .generators
                                .iter()
                                .map(|gen_id| inner.get_generator(*gen_id).unwrap())
                                .map(|gen| (gen.stream_id, gen.id, gen.name.clone()))
                                .collect_vec();

                            for (stream_id, id, name) in gens {
                                waves.add_generator(TransactionStreamRef::new_gen(
                                    stream_id,
                                    id,
                                    name.clone(),
                                ))
                            }
                        }
                    }
                    self.invalidate_draw_commands();
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
                if visible_items_len > 0 && idx.0 < visible_items_len {
                    waves.focused_item = Some(idx);
                } else {
                    error!(
                        "Can not focus variable {} because only {visible_items_len} variables are visible.", idx.0
                    );
                }
            }
            Message::ItemSelectRange(select_to) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if let Some(select_from) = waves.focused_item {
                    let range = if select_to.0 > select_from.0 {
                        select_from.0..=select_to.0
                    } else {
                        select_to.0..=select_from.0
                    };
                    for idx in range {
                        if let Some(item_ref) = waves.displayed_items_order.get(idx) {
                            waves.selected_items.insert(*item_ref);
                        }
                    }
                }
            }
            Message::ToggleItemSelected(idx) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if let Some(focused) = idx.or(waves.focused_item) {
                    let id = waves.displayed_items_order[focused.0];
                    if waves.selected_items.contains(&id) {
                        waves.selected_items.remove(&id);
                    } else {
                        waves.selected_items.insert(id);
                    }
                }
            }
            Message::UnfocusItem => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.focused_item = None;
                };
            }
            Message::RenameItem(vidx) => {
                self.save_current_canvas(format!(
                    "Rename item to {}",
                    self.sys.item_renaming_string.borrow()
                ));
                if let Some(waves) = self.waves.as_mut() {
                    let idx = vidx.or(waves.focused_item);
                    if let Some(idx) = idx {
                        self.rename_target = Some(idx);
                        *self.sys.item_renaming_string.borrow_mut() = waves
                            .displayed_items_order
                            .get(idx.0)
                            .and_then(|id| waves.displayed_items.get(id))
                            .map(displayed_item::DisplayedItem::name)
                            .unwrap_or_default();
                    }
                }
            }
            Message::MoveFocus(direction, count, select) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                let visible_items_len = waves.displayed_items.len();
                if visible_items_len > 0 {
                    self.count = None;
                    let new_focus_idx = match direction {
                        MoveDir::Up => waves
                            .focused_item
                            .map(|dii| dii.0)
                            .map_or(Some(visible_items_len - 1), |focused| {
                                Some(focused - count.clamp(0, focused))
                            }),
                        MoveDir::Down => waves.focused_item.map(|dii| dii.0).map_or(
                            Some((count - 1).clamp(0, visible_items_len - 1)),
                            |focused| Some((focused + count).clamp(0, visible_items_len - 1)),
                        ),
                    };

                    if let Some(idx) = new_focus_idx {
                        if select {
                            if let Some(focused) = waves.focused_item {
                                if let Some(focused_ref) =
                                    waves.displayed_items_order.get(focused.0)
                                {
                                    waves.selected_items.insert(*focused_ref);
                                }
                            }
                            if let Some(item_ref) = waves.displayed_items_order.get(idx) {
                                waves.selected_items.insert(*item_ref);
                            }
                        }
                        waves.focused_item = Some(DisplayedItemIndex::from(idx));
                    }
                }
            }
            Message::FocusTransaction(tx_ref, tx) => {
                if tx_ref.is_some() && tx.is_none() {
                    self.save_current_canvas(format!(
                        "Focus Transaction id: {}",
                        tx_ref.as_ref().unwrap().id
                    ));
                }
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                let invalidate = tx.is_none();
                waves.focused_transaction =
                    (tx_ref, tx.or_else(|| waves.focused_transaction.1.clone()));
                if invalidate {
                    self.invalidate_draw_commands();
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
            Message::RemoveItemByIndex(idx) => {
                let undo_msg = self
                    .waves
                    .as_ref()
                    .and_then(|waves| {
                        waves
                            .displayed_items_order
                            .get(idx.0)
                            .and_then(|id| waves.displayed_items.get(id))
                            .map(displayed_item::DisplayedItem::name)
                            .map(|name| format!("Remove item {name}"))
                    })
                    .unwrap_or("Remove one item".to_string());
                self.save_current_canvas(undo_msg);
                if let Some(waves) = self.waves.as_mut() {
                    if idx.0 < waves.displayed_items_order.len() {
                        let id = waves.displayed_items_order[idx.0];
                        waves.remove_displayed_item(id);
                    }
                }
            }
            Message::RemoveItems(items) => {
                let undo_msg = self
                    .waves
                    .as_ref()
                    .and_then(|waves| {
                        if items.len() == 1 {
                            items.first().and_then(|idx| {
                                waves
                                    .displayed_items_order
                                    .get(idx.0)
                                    .and_then(|id| waves.displayed_items.get(id))
                                    .map(|item| format!("Remove item {}", item.name()))
                            })
                        } else {
                            Some(format!("Remove {} items", items.len()))
                        }
                    })
                    .unwrap_or("".to_string());
                self.save_current_canvas(undo_msg);
                if let Some(waves) = self.waves.as_mut() {
                    let mut ordered_items = items.clone();
                    ordered_items.sort_unstable_by_key(|item| item.0);
                    ordered_items.dedup_by_key(|item| item.0);
                    for id in ordered_items {
                        waves.remove_displayed_item(id);
                    }
                    waves
                        .selected_items
                        .retain(|item_ref| !items.contains(item_ref));
                }
            }
            Message::MoveFocusedItem(direction, count) => {
                self.save_current_canvas(format!("Move item {direction}, {count}"));
                self.invalidate_draw_commands();
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                if let Some(DisplayedItemIndex(idx)) = waves.focused_item {
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
                                    waves.focused_item = Some((i - 1).into());
                                }
                            }
                            MoveDir::Down => {
                                for i in idx..(idx + count).clamp(0, visible_items_len - 1) {
                                    waves.displayed_items_order.swap(i, i + 1);
                                    waves.focused_item = Some((i + 1).into());
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
            Message::VariableFormatChange(displayed_field_ref, format) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                if !self
                    .sys
                    .translators
                    .all_translator_names()
                    .contains(&format.as_str())
                {
                    warn!("No translator {format}");
                    return;
                }

                let Some(DisplayedItem::Variable(displayed_variable)) =
                    waves.displayed_items.get_mut(&displayed_field_ref.item)
                else {
                    return;
                };

                if displayed_field_ref.field.is_empty() {
                    let Ok(meta) = waves
                        .inner
                        .as_waves()
                        .unwrap()
                        .variable_meta(&displayed_variable.variable_ref)
                        .map_err(|e| warn!("{e:#?}"))
                    else {
                        return;
                    };
                    let translator = self.sys.translators.get_translator(&format);
                    let new_info = translator.variable_info(&meta).unwrap();

                    displayed_variable.format = Some(format);
                    displayed_variable.info = new_info;
                } else {
                    displayed_variable
                        .field_formats
                        .retain(|ff| ff.field != displayed_field_ref.field);
                    displayed_variable.field_formats.push(FieldFormat {
                        field: displayed_field_ref.field,
                        format,
                    });
                }
                self.invalidate_draw_commands();
            }
            Message::ItemSelectionClear => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.selected_items.clear();
                }
            }
            Message::ItemColorChange(vidx, color_name) => {
                self.save_current_canvas(format!(
                    "Change item color to {}",
                    color_name.clone().unwrap_or("default".into())
                ));
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(DisplayedItemIndex(idx)) = vidx.or(waves.focused_item) {
                        waves.displayed_items_order.get(idx).map(|id| {
                            waves
                                .displayed_items
                                .entry(*id)
                                .and_modify(|item| item.set_color(color_name.clone()))
                        });
                    }
                    if vidx.is_none() {
                        for idx in waves.selected_items.iter() {
                            waves
                                .displayed_items
                                .entry(*idx)
                                .and_modify(|item| item.set_color(color_name.clone()));
                        }
                    }
                };
            }
            Message::ItemNameChange(vidx, name) => {
                self.save_current_canvas(format!(
                    "Change item name to {}",
                    name.clone().unwrap_or("default".into())
                ));
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(DisplayedItemIndex(idx)) = vidx.or(waves.focused_item) {
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
                self.save_current_canvas(format!(
                    "Change item background color to {}",
                    color_name.clone().unwrap_or("default".into())
                ));
                if let Some(waves) = self.waves.as_mut() {
                    if let Some(DisplayedItemIndex(idx)) = vidx.or(waves.focused_item) {
                        waves.displayed_items_order.get(idx).map(|id| {
                            waves
                                .displayed_items
                                .entry(*id)
                                .and_modify(|item| item.set_background_color(color_name.clone()))
                        });
                    }
                    if vidx.is_none() {
                        for idx in waves.selected_items.iter() {
                            waves
                                .displayed_items
                                .entry(*idx)
                                .and_modify(|item| item.set_background_color(color_name.clone()));
                        }
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
                        if let Some(vp) = waves.viewports.first() {
                            let num_timestamps = waves.num_timestamps();
                            waves.cursor = if next {
                                Some(vp.left_edge_time(&num_timestamps))
                            } else {
                                Some(vp.right_edge_time(&num_timestamps))
                            };
                        }
                    }
                    waves.set_cursor_at_transition(next, variable, skip_zero);
                    let moved = waves.go_to_cursor_if_not_in_view();
                    if moved {
                        self.invalidate_draw_commands();
                    }
                }
            }
            Message::MoveTransaction { next } => {
                let undo_msg = if next {
                    "Move to next transaction"
                } else {
                    "Move to previous transaction"
                };
                self.save_current_canvas(undo_msg.to_string());
                if let Some(waves) = &mut self.waves {
                    if let Some(inner) = waves.inner.as_transactions() {
                        let mut transactions = waves
                            .displayed_items_order
                            .iter()
                            .map(|item_id| {
                                let item = &waves.displayed_items[item_id];
                                match item {
                                    DisplayedItem::Stream(s) => {
                                        let stream_ref = &s.transaction_stream_ref;
                                        let stream_id = stream_ref.stream_id;
                                        if let Some(gen_id) = stream_ref.gen_id {
                                            inner.get_transactions_from_generator(gen_id)
                                        } else {
                                            inner.get_transactions_from_stream(stream_id)
                                        }
                                    }
                                    _ => vec![],
                                }
                            })
                            .flatten()
                            .collect_vec();

                        transactions.sort();
                        let tx = if let Some(focused_tx) = &waves.focused_transaction.0 {
                            let next_id = transactions
                                .iter()
                                .enumerate()
                                .find(|(_, tx)| **tx == focused_tx.id)
                                .map(|(vec_idx, _)| {
                                    if next {
                                        if vec_idx + 1 < transactions.len() {
                                            vec_idx + 1
                                        } else {
                                            transactions.len() - 1
                                        }
                                    } else {
                                        if vec_idx as i32 - 1 > 0 {
                                            vec_idx - 1
                                        } else {
                                            0
                                        }
                                    }
                                })
                                .unwrap_or(next.then_some(transactions.len() - 1).unwrap_or(0));
                            Some(TransactionRef {
                                id: *transactions.get(next_id).unwrap(),
                            })
                        } else if !transactions.is_empty() {
                            Some(TransactionRef {
                                id: *transactions.get(0).unwrap(),
                            })
                        } else {
                            None
                        };
                        waves.focused_transaction = (tx, waves.focused_transaction.1.clone());
                    }
                    self.invalidate_draw_commands();
                }
            }
            Message::ResetVariableFormat(displayed_field_ref) => {
                if let Some(DisplayedItem::Variable(displayed_variable)) = self
                    .waves
                    .as_mut()
                    .and_then(|waves| waves.displayed_items.get_mut(&displayed_field_ref.item))
                {
                    if displayed_field_ref.field.is_empty() {
                        displayed_variable.format = None;
                    } else {
                        displayed_variable
                            .field_formats
                            .retain(|ff| ff.field != displayed_field_ref.field);
                    }
                    self.invalidate_draw_commands();
                }
            }
            Message::CursorSet(new) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.cursor = Some(new);
                }
            }
            Message::LoadFile(filename, load_options) => {
                self.load_from_file(filename, load_options).ok();
            }
            Message::LoadWaveformFileFromUrl(url, load_options) => {
                self.load_wave_from_url(url, load_options);
            }
            Message::LoadFromData(data, load_options) => {
                self.load_from_data(data, load_options).ok();
            }
            #[cfg(feature = "python")]
            Message::LoadPythonTranslator(filename) => {
                try_log_error!(
                    self.sys.translators.load_python_translator(filename),
                    "Error loading Python translator",
                )
            }
            Message::LoadSpadeTranslator { top, state } => {
                #[cfg(feature = "spade")]
                {
                    let sender = self.sys.channels.msg_sender.clone();
                    perform_work(move || {
                        #[cfg(feature = "spade")]
                        SpadeTranslator::init(&top, &state, sender);
                    });
                };
                #[cfg(not(feature = "spade"))]
                {
                    info!(
                        "Surfer is not compiled with spade support, ignoring LoadSpadeTranslator"
                    );
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Message::ConnectToCxxrtl(url) => self.connect_to_cxxrtl(url, false),
            Message::SurferServerStatus(_start, server, status) => {
                self.server_status_to_progress(server, status);
            }
            Message::FileDropped(dropped_file) => {
                self.load_from_dropped(dropped_file)
                    .map_err(|e| error!("{e:#?}"))
                    .ok();
            }
            Message::WaveHeaderLoaded(start, source, load_options, header) => {
                // for files using the `wellen` backend, we load the header before parsing the body
                info!(
                    "Loaded the hierarchy and meta-data of {source} in {:?}",
                    start.elapsed()
                );
                match header {
                    HeaderResult::Local(header) => {
                        // register waveform as loaded (but with no variable info yet!)
                        let shared_hierarchy = Arc::new(header.hierarchy);
                        let new_waves =
                            Box::new(WaveContainer::new_waveform(shared_hierarchy.clone()));
                        self.on_waves_loaded(
                            source.clone(),
                            convert_format(header.file_format),
                            new_waves,
                            load_options,
                        );
                        // start parsing of the body
                        self.load_wave_body(source, header.body, header.body_len, shared_hierarchy);
                    }
                    HeaderResult::Remote(hierarchy, file_format, server) => {
                        // register waveform as loaded (but with no variable info yet!)
                        let new_waves = Box::new(WaveContainer::new_remote_waveform(
                            server.clone(),
                            hierarchy.clone(),
                        ));
                        self.on_waves_loaded(
                            source.clone(),
                            convert_format(file_format),
                            new_waves,
                            load_options,
                        );
                        // body is already being parsed on the server, we need to request the time table though
                        Self::get_time_table_from_server(
                            self.sys.channels.msg_sender.clone(),
                            server,
                        );
                    }
                }
            }
            Message::WaveBodyLoaded(start, source, body) => {
                // for files using the `wellen` backend, parse the body in a second step
                info!("Loaded the body of {source} in {:?}", start.elapsed());
                self.sys.progress_tracker = None;
                let waves = self
                    .waves
                    .as_mut()
                    .expect("Waves should be loaded at this point!");
                // add source and time table
                let maybe_cmd = waves
                    .inner
                    .as_waves_mut()
                    .unwrap()
                    .wellen_add_body(body)
                    .unwrap_or_else(|err| {
                        error!("While getting commands to lazy-load signals: {err:?}");
                        None
                    });
                // Pre-load parameters
                let param_cmd = waves
                    .inner
                    .as_waves_mut()
                    .unwrap()
                    .load_parameters()
                    .unwrap_or_else(|err| {
                        error!("While getting commands to lazy-load parameters: {err:?}");
                        None
                    });
                // update viewports, now that we have the time table
                waves.update_viewports();
                // make sure we redraw
                self.invalidate_draw_commands();
                // start loading parameters
                if let Some(cmd) = param_cmd {
                    self.load_variables(cmd);
                }
                // start loading variables
                if let Some(cmd) = maybe_cmd {
                    self.load_variables(cmd);
                }
            }
            Message::SignalsLoaded(start, res) => {
                info!("Loaded {} variables in {:?}", res.len(), start.elapsed());
                self.sys.progress_tracker = None;
                let waves = self
                    .waves
                    .as_mut()
                    .expect("Waves should be loaded at this point!");
                match waves.inner.as_waves_mut().unwrap().on_signals_loaded(res) {
                    Err(err) => error!("{err:?}"),
                    Ok(Some(cmd)) => self.load_variables(cmd),
                    _ => {}
                }
                // make sure we redraw since now more variable data is available
                self.invalidate_draw_commands();
            }
            Message::WavesLoaded(filename, format, new_waves, load_options) => {
                self.on_waves_loaded(filename, format, new_waves, load_options);
                // here, the body and thus the number of timestamps is already loaded!
                self.waves.as_mut().unwrap().update_viewports();
                self.sys.progress_tracker = None;
            }
            Message::TransactionStreamsLoaded(filename, format, new_ftr, loaded_options) => {
                self.on_transaction_streams_loaded(filename, format, new_ftr, loaded_options);
                self.waves.as_mut().unwrap().update_viewports();
            }
            Message::BlacklistTranslator(idx, translator) => {
                self.blacklisted_translators.insert((idx, translator));
            }
            Message::Error(e) => {
                error!("{e:?}");
                self.show_logs = true;
            }
            Message::TranslatorLoaded(t) => {
                info!("Translator {} loaded", t.name());
                self.sys.translators.add_or_replace(AnyTranslator::Full(t));
            }
            Message::ToggleSidePanel => {
                let new = match self.show_hierarchy {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_hierarchy(),
                };
                self.show_hierarchy = Some(new);
            }
            Message::ToggleMenu => {
                let new = match self.show_menu {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_menu(),
                };
                self.show_menu = Some(new);
            }
            Message::ToggleToolbar => {
                let new = match self.show_toolbar {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_toolbar(),
                };
                self.show_toolbar = Some(new);
            }
            Message::ToggleEmptyScopes => {
                let new = match self.show_empty_scopes {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_empty_scopes(),
                };
                self.show_empty_scopes = Some(new);
            }
            Message::ToggleParametersInScopes => {
                let new = match self.show_parameters_in_scopes {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_parameters_in_scopes(),
                };
                self.show_parameters_in_scopes = Some(new);
            }
            Message::ToggleStatusbar => {
                let new = match self.show_statusbar {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_statusbar(),
                };
                self.show_statusbar = Some(new);
            }
            Message::ToggleTickLines => {
                let new = match self.show_ticks {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_ticks(),
                };
                self.show_ticks = Some(new);
            }
            Message::ToggleVariableTooltip => {
                let new = match self.show_tooltip {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_tooltip(),
                };
                self.show_tooltip = Some(new);
            }
            Message::ToggleOverview => {
                let new = match self.show_overview {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_overview(),
                };
                self.show_overview = Some(new);
            }
            Message::ToggleDirection => {
                let new = match self.show_variable_direction {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_variable_direction(),
                };
                self.show_variable_direction = Some(new);
            }
            Message::ToggleIndices => {
                let new = match self.show_variable_indices {
                    Some(prev) => !prev,
                    None => !self.config.layout.show_variable_indices(),
                };
                self.show_variable_indices = Some(new);
                if let Some(waves) = self.waves.as_mut() {
                    waves.display_variable_indices = new;
                    waves.compute_variable_display_names();
                }
            }
            Message::ShowCommandPrompt(text) => {
                if let Some(init_text) = text {
                    self.sys.command_prompt.new_cursor_pos = Some(init_text.len());
                    *self.sys.command_prompt_text.borrow_mut() = init_text;
                    self.sys.command_prompt.visible = true;
                } else {
                    *self.sys.command_prompt_text.borrow_mut() = "".to_string();
                    self.sys.command_prompt.suggestions = vec![];
                    self.sys.command_prompt.selected =
                        self.sys.command_prompt.previous_commands.len();
                    self.sys.command_prompt.visible = false;
                }
            }
            Message::FileDownloaded(url, bytes, load_options) => {
                self.load_from_bytes(WaveSource::Url(url), bytes.to_vec(), load_options)
            }
            Message::SetConfigFromString(s) => {
                // FIXME think about a structured way to collect errors
                if let Ok(config) =
                    SurferConfig::new_from_toml(&s).with_context(|| "Failed to load config file")
                {
                    self.config = config;
                    if let Some(ctx) = &self.sys.context.as_ref() {
                        ctx.set_visuals(self.get_visuals())
                    }
                }
            }
            Message::ReloadConfig => {
                // FIXME think about a structured way to collect errors
                if let Ok(config) =
                    SurferConfig::new(false).with_context(|| "Failed to load config file")
                {
                    self.sys.translators = all_translators();
                    self.config = config;
                    if let Some(ctx) = &self.sys.context.as_ref() {
                        ctx.set_visuals(self.get_visuals());
                    }
                }
            }
            Message::ReloadWaveform(keep_unavailable) => {
                let Some(waves) = &self.waves else { return };
                match &waves.source {
                    WaveSource::File(filename) => {
                        self.load_from_file(
                            filename.clone(),
                            LoadOptions {
                                keep_variables: true,
                                keep_unavailable,
                            },
                        )
                        .ok();
                    }
                    WaveSource::Data => {} // can't reload
                    #[cfg(not(target_arch = "wasm32"))]
                    WaveSource::CxxrtlTcp(..) => {} // can't reload
                    WaveSource::DragAndDrop(filename) => {
                        filename.clone().and_then(|filename| {
                            self.load_from_file(
                                filename,
                                LoadOptions {
                                    keep_variables: true,
                                    keep_unavailable,
                                },
                            )
                            .ok()
                        });
                    }
                    WaveSource::Url(url) => {
                        self.load_wave_from_url(
                            url.clone(),
                            LoadOptions {
                                keep_variables: true,
                                keep_unavailable,
                            },
                        );
                    }
                };

                for translator in self.sys.translators.all_translators() {
                    translator.reload(self.sys.channels.msg_sender.clone());
                }
            }
            Message::SuggestReloadWaveform => match self.config.autoreload_files {
                Some(true) => {
                    self.update(Message::ReloadWaveform(true));
                }
                Some(false) => {}
                None => self.show_reload_suggestion = Some(ReloadWaveformDialog::default()),
            },
            Message::CloseReloadWaveformDialog {
                reload_file,
                do_not_show_again,
            } => {
                if do_not_show_again {
                    // FIXME: This is currently for one session only, but could be persisted in
                    // some setting.
                    self.config.autoreload_files = Some(reload_file);
                }
                self.show_reload_suggestion = None;
                if reload_file {
                    self.update(Message::ReloadWaveform(true));
                }
            }
            Message::UpdateReloadWaveformDialog(dialog) => {
                self.show_reload_suggestion = Some(dialog);
            }
            Message::RemovePlaceholders => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.remove_placeholders();
                }
            }
            Message::SetClockHighlightType(new_type) => {
                self.config.default_clock_highlight_type = new_type;
            }
            Message::SetMarker { id, time } => {
                self.save_current_canvas(format!("Set marker to {time}"));
                if let Some(waves) = self.waves.as_mut() {
                    waves.set_marker_position(id, &time);
                };
            }
            Message::MoveMarkerToCursor(idx) => {
                self.save_current_canvas("Move marker".into());
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
                if let Some(DisplayedItemIndex(idx)) = vidx.or(waves.focused_item) {
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
                *self.sys.command_prompt_text.borrow_mut() = String::new();
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
                self.sys.command_prompt.new_selection =
                    Some(if self.sys.command_prompt_text.borrow().is_empty() {
                        self.sys.command_prompt.previous_commands.len().clamp(0, 3)
                    } else {
                        0
                    });
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
            #[cfg(feature = "python")]
            Message::OpenPythonPluginDialog => {
                self.open_python_file_dialog();
            }
            #[cfg(feature = "python")]
            Message::ReloadPythonPlugin => {
                try_log_error!(
                    self.sys.translators.reload_python_translator(),
                    "Error reloading Python translator"
                );
                self.invalidate_draw_commands();
            }
            Message::SaveStateFile(path) => self.save_state_file(path),
            Message::LoadStateFile(path) => self.load_state_file(path),
            Message::LoadState(state, path) => self.load_state(state, path),
            Message::SetStateFile(path) => {
                // since in wasm we can't support "save", only "save as" - never set the `state_file`
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.state_file = Some(path);
                }
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
                    self.sys.continuous_redraw = false;
                }
                self.show_performance = s;
            }
            Message::SetContinuousRedraw(s) => self.sys.continuous_redraw = s,
            Message::SetDragStart(pos) => self.sys.gesture_start_location = pos,
            Message::SetFilterFocused(s) => self.variable_name_filter_focused = s,
            Message::SetVariableNameFilterType(variable_name_filter_type) => {
                self.variable_name_filter_type = variable_name_filter_type;
            }
            Message::SetVariableNameFilterCaseInsensitive(s) => {
                self.variable_name_filter_case_insensitive = s;
            }
            Message::SetUIZoomFactor(scale) => {
                if let Some(ctx) = &mut self.sys.context.as_ref() {
                    ctx.set_zoom_factor(scale);
                }
                self.ui_zoom_factor = Some(scale);
            }
            Message::SelectPrevCommand => {
                self.sys.command_prompt.new_selection = self
                    .sys
                    .command_prompt
                    .new_selection
                    .or(Some(self.sys.command_prompt.selected))
                    .map(|idx| idx.saturating_sub(1).max(0));
            }
            Message::SelectNextCommand => {
                self.sys.command_prompt.new_selection = self
                    .sys
                    .command_prompt
                    .new_selection
                    .or(Some(self.sys.command_prompt.selected))
                    .map(|idx| {
                        idx.saturating_add(1)
                            .min(self.sys.command_prompt.suggestions.len().saturating_sub(1))
                    });
            }
            Message::SetHierarchyStyle(style) => self.config.layout.hierarchy_style = style,
            Message::SetArrowKeyBindings(bindings) => {
                self.config.behavior.arrow_key_bindings = bindings;
            }
            Message::InvalidateDrawCommands => self.invalidate_draw_commands(),
            Message::UnpauseSimulation => {
                if let Some(waves) = &self.waves {
                    waves.inner.as_waves().unwrap().unpause_simulation();
                }
            }
            Message::PauseSimulation => {
                if let Some(waves) = &self.waves {
                    waves.inner.as_waves().unwrap().pause_simulation();
                }
            }
            Message::Batch(messages) => {
                for message in messages {
                    self.update(message);
                }
            }
            Message::AddDraggedVariables(variables) => {
                if self.waves.is_some() {
                    self.waves.as_mut().unwrap().focused_item = None;
                    let waves = self.waves.as_mut().unwrap();
                    if let Some(DisplayedItemIndex(target_idx)) = self.drag_target_idx {
                        let variables_len = variables.len() - 1;
                        let items_len = waves.displayed_items_order.len();
                        if let Some(cmd) = waves.add_variables(&self.sys.translators, variables) {
                            self.load_variables(cmd);
                        }

                        for i in 0..=variables_len {
                            let to_insert = self
                                .waves
                                .as_mut()
                                .unwrap()
                                .displayed_items_order
                                .remove(items_len + i);
                            self.waves
                                .as_mut()
                                .unwrap()
                                .displayed_items_order
                                .insert(target_idx + i, to_insert);
                        }
                    } else {
                        if let Some(cmd) = self
                            .waves
                            .as_mut()
                            .unwrap()
                            .add_variables(&self.sys.translators, variables)
                        {
                            self.load_variables(cmd);
                        }
                    }
                    self.invalidate_draw_commands();
                }
                self.drag_source_idx = None;
                self.drag_target_idx = None;
            }
            Message::VariableDragStarted(vidx) => {
                self.drag_started = true;
                self.drag_source_idx = Some(vidx);
                self.drag_target_idx = None;
            }
            Message::VariableDragTargetChanged(vidx) => {
                self.drag_target_idx = Some(vidx);
            }
            Message::VariableDragFinished => {
                self.drag_started = false;

                // reordering
                if let (
                    Some(DisplayedItemIndex(source_vidx)),
                    Some(DisplayedItemIndex(target_vidx)),
                ) = (self.drag_source_idx, self.drag_target_idx)
                {
                    self.save_current_canvas("Drag item".to_string());
                    self.invalidate_draw_commands();
                    let Some(waves) = self.waves.as_mut() else {
                        return;
                    };
                    let visible_items_len = waves.displayed_items.len();
                    if visible_items_len > 0 {
                        let old_idx = waves.displayed_items_order.remove(source_vidx);
                        if waves.displayed_items_order.len() < target_vidx {
                            waves.displayed_items_order.push(old_idx);
                        } else {
                            waves.displayed_items_order.insert(target_vidx, old_idx);
                        }

                        // carry focused item when moving it
                        if waves.focused_item.is_some_and(|f| f.0 == source_vidx) {
                            waves.focused_item = Some(target_vidx.into());
                        }
                    }
                }
                self.drag_source_idx = None;
                self.drag_target_idx = None;
            }
            Message::VariableValueToClipbord(vidx) => {
                if let Some(waves) = &self.waves {
                    if let Some(DisplayedItemIndex(vidx)) = vidx.or(waves.focused_item) {
                        if let Some(DisplayedItem::Variable(_displayed_variable)) = waves
                            .displayed_items_order
                            .get(vidx)
                            .and_then(|id| waves.displayed_items.get(id))
                        {
                            let field_ref =
                                (*waves.displayed_items_order.get(vidx).unwrap()).into();
                            let variable_value = self.get_variable_value(
                                waves,
                                &field_ref,
                                &waves.cursor.as_ref().and_then(num::BigInt::to_biguint),
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
            Message::SetViewportStrategy(s) => {
                if let Some(waves) = &mut self.waves {
                    for vp in &mut waves.viewports {
                        vp.move_strategy = s
                    }
                }
            }
            Message::Undo(count) => {
                if let Some(waves) = &mut self.waves {
                    for _ in 0..count {
                        if let Some(prev_state) = self.sys.undo_stack.pop() {
                            self.sys
                                .redo_stack
                                .push(State::current_canvas_state(waves, prev_state.message));
                            waves.focused_item = prev_state.focused_item;
                            waves.focused_transaction = prev_state.focused_transaction;
                            waves.selected_items = prev_state.selected_items;
                            waves.displayed_items_order = prev_state.displayed_item_order;
                            waves.displayed_items = prev_state.displayed_items;
                            waves.markers = prev_state.markers;
                        } else {
                            break;
                        }
                    }
                    self.invalidate_draw_commands();
                }
            }
            Message::Redo(count) => {
                if let Some(waves) = &mut self.waves {
                    for _ in 0..count {
                        if let Some(prev_state) = self.sys.redo_stack.pop() {
                            self.sys
                                .undo_stack
                                .push(State::current_canvas_state(waves, prev_state.message));
                            waves.focused_item = prev_state.focused_item;
                            waves.focused_transaction = prev_state.focused_transaction;
                            waves.displayed_items_order = prev_state.displayed_item_order;
                            waves.displayed_items = prev_state.displayed_items;
                            waves.markers = prev_state.markers;
                        } else {
                            break;
                        }
                    }
                    self.invalidate_draw_commands();
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
            Message::SelectTheme(theme_name) => {
                if let Ok(theme) =
                    SurferTheme::new(theme_name).with_context(|| "Failed to set theme")
                {
                    self.config.theme = theme;
                    if let Some(ctx) = &self.sys.context.as_ref() {
                        ctx.set_visuals(self.get_visuals());
                    }
                }
            }
            Message::AsyncDone(_) => (),
            Message::AddGraphic(id, g) => {
                if let Some(waves) = &mut self.waves {
                    waves.graphics.insert(id, g);
                }
            }
            Message::RemoveGraphic(id) => {
                if let Some(waves) = &mut self.waves {
                    waves.graphics.retain(|k, _| k != &id)
                }
            }
            Message::ExpandDrawnItem { item, levels } => {
                self.sys.items_to_expand.borrow_mut().push((item, levels))
            }
            Message::AddCharToPrompt(c) => *self.sys.char_to_add_to_prompt.borrow_mut() = Some(c),
        }
    }

    fn add_scope(&mut self, scope: ScopeRef, recursive: bool) {
        let Some(waves) = self.waves.as_mut() else {
            warn!("Adding scope without waves loaded");
            return;
        };

        let wave_cont = waves.inner.as_waves().unwrap();

        let children = wave_cont.child_scopes(&scope);
        let variables = wave_cont
            .variables_in_scope(&scope)
            .iter()
            .sorted_by(|a, b| numeric_sort::cmp(&a.name, &b.name))
            .cloned()
            .collect_vec();

        let variable_len = variables.len();
        let items_len = waves.displayed_items_order.len();
        if let Some(cmd) = waves.add_variables(&self.sys.translators, variables) {
            self.load_variables(cmd);
        }
        if let (Some(DisplayedItemIndex(target_idx)), Some(_)) =
            (self.drag_target_idx, self.drag_source_idx)
        {
            for i in 0..variable_len {
                let to_insert = self
                    .waves
                    .as_mut()
                    .unwrap()
                    .displayed_items_order
                    .remove(items_len + i);
                self.waves
                    .as_mut()
                    .unwrap()
                    .displayed_items_order
                    .insert(target_idx + i, to_insert);
            }
        }

        if recursive {
            if let Ok(children) = children {
                for child in children {
                    self.add_scope(child, true);
                }
            }
        }
        self.invalidate_draw_commands();
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

        let (new_wave, load_commands) = if load_options.keep_variables && self.waves.is_some() {
            self.waves.take().unwrap().update_with_waves(
                new_waves,
                filename,
                format,
                &self.sys.translators,
                load_options.keep_unavailable,
            )
        } else if let Some(old) = self.previous_waves.take() {
            old.update_with_waves(
                new_waves,
                filename,
                format,
                &self.sys.translators,
                load_options.keep_unavailable,
            )
        } else {
            (
                WaveData {
                    inner: DataContainer::Waves(*new_waves),
                    source: filename,
                    format,
                    active_scope: None,
                    displayed_items_order: vec![],
                    displayed_items: HashMap::new(),
                    viewports,
                    cursor: None,
                    markers: HashMap::new(),
                    focused_item: None,
                    focused_transaction: (None, None),
                    selected_items: HashSet::new(),
                    default_variable_name_type: self.config.default_variable_name_type,
                    display_variable_indices: self.show_variable_indices(),
                    scroll_offset: 0.,
                    drawing_infos: vec![],
                    top_item_draw_offset: 0.,
                    total_height: 0.,
                    display_item_ref_counter: 0,
                    old_num_timestamps: None,
                    graphics: HashMap::new(),
                },
                None,
            )
        };
        if let Some(cmd) = load_commands {
            self.load_variables(cmd);
        }
        self.invalidate_draw_commands();

        // Set time unit to the file time unit before consuming new_wave
        self.wanted_timeunit = new_wave.inner.metadata().timescale.unit;

        self.waves = Some(new_wave);
    }

    fn on_transaction_streams_loaded(
        &mut self,
        filename: WaveSource,
        format: WaveFormat,
        new_ftr: TransactionContainer,
        _loaded_options: LoadOptions,
    ) {
        info!("Transaction streams are loaded.");

        let viewport = Viewport::new();
        let viewports = [viewport].to_vec();

        let new_transaction_streams = WaveData {
            inner: Transactions(new_ftr),
            source: filename,
            format,
            active_scope: None,
            displayed_items_order: vec![],
            displayed_items: HashMap::new(),
            viewports,
            cursor: None,
            markers: HashMap::new(),
            focused_item: None,
            focused_transaction: (None, None),
            selected_items: HashSet::new(),
            default_variable_name_type: self.config.default_variable_name_type,
            display_variable_indices: self.show_variable_indices(),
            scroll_offset: 0.,
            drawing_infos: vec![],
            top_item_draw_offset: 0.,
            total_height: 0.,
            display_item_ref_counter: 0,
            old_num_timestamps: None,
            graphics: HashMap::new(),
        };

        self.invalidate_draw_commands();

        self.config.theme.alt_frequency = 0;
        self.wanted_timeunit = new_transaction_streams.inner.metadata().timescale.unit;
        self.waves = Some(new_transaction_streams);
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

    /// After user messages are addressed, we try to execute batch commands as they are ready to run
    fn handle_batch_commands(&mut self) {
        // we only execute commands while we aren't waiting for background operations to complete
        while self.can_start_batch_command() {
            if let Some(cmd) = self.sys.batch_commands.pop_front() {
                info!("Applying startup command: {cmd:?}");
                self.update(cmd);
            } else {
                break; // no more messages
            }
        }

        // if there are no messages and all operations have completed, we are done
        if !self.sys.batch_commands_completed
            && self.sys.batch_commands.is_empty()
            && self.can_start_batch_command()
        {
            self.sys.batch_commands_completed = true;
        }
    }

    /// Returns whether it is OK to start a new batch command.
    fn can_start_batch_command(&self) -> bool {
        // if the progress tracker is none -> all operations have completed
        self.sys.progress_tracker.is_none()
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

    fn encode_state(&self) -> Option<String> {
        let opt = ron::Options::default();
        opt.to_string_pretty(self, PrettyConfig::default())
            .context("Failed to encode state")
            .map_err(|e| error!("Failed to encode state. {e:#?}"))
            .ok()
    }

    fn load_state(&mut self, mut loaded_state: crate::State, path: Option<PathBuf>) {
        // first swap everything, fix special cases afterwards
        mem::swap(self, &mut loaded_state);

        // system state is not exported and instance specific, swap back
        // we need to do this before fixing wave files which e.g. use the translator list
        mem::swap(&mut self.sys, &mut loaded_state.sys);
        // the config is also not exported and instance specific, swap back
        mem::swap(&mut self.config, &mut loaded_state.config);

        // swap back waves for inner, source, format since we want to keep the file
        // fix up all wave references from paths if a wave is loaded
        mem::swap(&mut loaded_state.waves, &mut self.waves);
        let load_commands = if let (Some(waves), Some(new_waves)) =
            (&mut self.waves, &mut loaded_state.waves)
        {
            mem::swap(&mut waves.active_scope, &mut new_waves.active_scope);
            let items = std::mem::take(&mut new_waves.displayed_items);
            let items_order = std::mem::take(&mut new_waves.displayed_items_order);
            let load_commands = waves.update_with_items(&items, items_order, &self.sys.translators);

            mem::swap(&mut waves.viewports, &mut new_waves.viewports);
            mem::swap(&mut waves.cursor, &mut new_waves.cursor);
            mem::swap(&mut waves.markers, &mut new_waves.markers);
            mem::swap(&mut waves.focused_item, &mut new_waves.focused_item);
            waves.default_variable_name_type = new_waves.default_variable_name_type;
            waves.scroll_offset = new_waves.scroll_offset;
            load_commands
        } else {
            None
        };
        if let Some(load_commands) = load_commands {
            self.load_variables(load_commands);
        };

        // reset drag to avoid confusion
        self.drag_started = false;
        self.drag_source_idx = None;
        self.drag_target_idx = None;

        // reset previous_waves & count to prevent unintuitive state here
        self.previous_waves = None;
        self.count = None;

        // use just loaded path since path is not part of the export as it might have changed anyways
        self.state_file = path;
        self.rename_target = None;

        self.invalidate_draw_commands();
        if let Some(waves) = &mut self.waves {
            waves.update_viewports();
        }
    }

    /// Returns true if the waveform and all requested signals have been loaded.
    /// Used for testing to make sure the GUI is at its final state before taking a
    /// snapshot.
    pub fn waves_fully_loaded(&self) -> bool {
        self.waves
            .as_ref()
            .is_some_and(|w| w.inner.is_fully_loaded())
    }

    /// Returns true once all batch commands have been completed and their effects are all executed.
    pub fn batch_commands_completed(&self) -> bool {
        debug_assert!(
            self.sys.batch_commands_completed || !self.sys.batch_commands.is_empty(),
            "completed implies no commands"
        );
        self.sys.batch_commands_completed
    }

    fn parse_startup_commands<I: IntoIterator<Item = String>>(&mut self, cmds: I) -> Vec<Message> {
        trace!("Parsing startup commands");
        let parsed = cmds
            .into_iter()
            // Add line numbers
            .enumerate()
            // trace
            .map(|(no, line)| {
                trace!("{no: >2} {line}");
                (no, line)
            })
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
                    .ok()
            })
            .collect::<Vec<_>>();

        parsed
    }

    /// Returns the current canvas state
    fn current_canvas_state(waves: &WaveData, message: String) -> CanvasState {
        CanvasState {
            message,
            focused_item: waves.focused_item,
            focused_transaction: waves.focused_transaction.clone(),
            selected_items: waves.selected_items.clone(),
            displayed_item_order: waves.displayed_items_order.clone(),
            displayed_items: waves.displayed_items.clone(),
            markers: waves.markers.clone(),
        }
    }

    /// Push the current canvas state to the undo stack
    fn save_current_canvas(&mut self, message: String) {
        if let Some(waves) = &self.waves {
            self.sys
                .undo_stack
                .push(State::current_canvas_state(waves, message));

            if self.sys.undo_stack.len() > self.config.undo_stack_size {
                self.sys.undo_stack.remove(0);
            }
            self.sys.redo_stack.clear();
        }
    }
}

pub struct StateWrapper(Arc<RwLock<State>>);
impl App for StateWrapper {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        App::update(&mut *self.0.write().unwrap(), ctx, frame)
    }
}
