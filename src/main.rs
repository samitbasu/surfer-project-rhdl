mod benchmark;
mod command_prompt;
mod commands;
mod config;
mod cursor;
mod displayed_item;
mod fast_wave_container;
mod help;
mod keys;
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

use camino::Utf8PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
use color_eyre::eyre::anyhow;
use color_eyre::eyre::Context;
use color_eyre::Result;
use displayed_item::DisplayedItem;
use displayed_item::DisplayedSignal;
#[cfg(not(target_arch = "wasm32"))]
use eframe::egui;
use eframe::egui::style::Selection;
use eframe::egui::style::WidgetVisuals;
use eframe::egui::style::Widgets;
use eframe::egui::DroppedFile;
use eframe::egui::Visuals;
use eframe::emath;
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
use log::error;
use log::info;
use log::warn;
use message::Message;
use num::BigInt;
use num::ToPrimitive;
use progress_streams::ProgressReader;
use serde::Deserialize;
use signal_filter::SignalFilterType;
use signal_name_type::SignalNameType;
use translation::spade::SpadeTranslator;
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
    pub waves: Option<WaveSource>,
}

impl StartupParams {
    #[allow(dead_code)] // NOTE: Only used in wasm version
    pub fn empty() -> Self {
        Self {
            spade_state: None,
            spade_top: None,
            waves: None,
        }
    }

    #[allow(dead_code)] // NOTE: Only used in wasm version
    pub fn vcd_from_url(url: Option<String>) -> Self {
        Self {
            spade_state: None,
            spade_top: None,
            waves: url.map(WaveSource::Url),
        }
    }

    #[allow(dead_code)] // NOTE: Only used in desktop version
    pub fn from_args(args: Args) -> Self {
        Self {
            spade_state: args.spade_state,
            spade_top: args.spade_top,
            waves: args.vcd_file.map(WaveSource::File),
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
            let Some(signal_ref) = new_wave.displayed_items.iter().find_map(|di| match di {
                DisplayedItem::Signal(DisplayedSignal { signal_ref, .. }) => Some(signal_ref),
                _ => None,
            }) else {
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

#[derive(Debug)]
pub enum OpenMode {
    Open,
    Switch,
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
}
