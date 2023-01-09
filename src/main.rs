mod signal_canvas;
mod translation;
mod view;
mod viewport;

use camino::Utf8PathBuf;
use clap::Parser;
use color_eyre::Result;
use color_eyre::eyre::Context;
use color_eyre::eyre::anyhow;
use eframe::egui;
use eframe::epaint::Vec2;
use fastwave_backend::parse_vcd;
use fastwave_backend::ScopeIdx;
use fastwave_backend::SignalIdx;
use fastwave_backend::VCD;
use num::bigint::ToBigInt;
use num::BigInt;
use num::BigRational;
use num::FromPrimitive;
use num::ToPrimitive;
use pyo3::append_to_inittab;

use translation::pytranslator::surfer;
use translation::SignalInfo;
use translation::Translator;
use translation::TranslatorList;
use viewport::Viewport;

use std::collections::HashMap;
use std::fs::File;

#[derive(clap::Parser)]
struct Args {
    vcd_file: Utf8PathBuf,
}

fn main() -> Result<()> {
    color_eyre::install()?;

    // Load python modules we deinfe in this crate
    append_to_inittab!(surfer);

    let args = Args::parse();

    let state = State::new(args)?;

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1920., 1080.)),
        ..Default::default()
    };
    eframe::run_native("My egui App", options, Box::new(|_cc| Box::new(state)));

    Ok(())
}

struct State {
    vcd: Option<VCD>,
    active_scope: Option<ScopeIdx>,
    signals: Vec<(SignalIdx, SignalInfo)>,
    /// The offset of the left side of the wave window in signal timestamps.
    viewport: Viewport,
    control_key: bool,
    num_timestamps: BigInt,
    /// Which translator to use for each signal
    signal_format: HashMap<SignalIdx, String>,
    translators: TranslatorList,
    cursor: BigInt,
}

#[derive(Debug, Clone)]
enum Message {
    HierarchyClick(ScopeIdx),
    AddSignal(SignalIdx),
    SignalFormatChange(SignalIdx, String),
    CanvasScroll {
        delta: Vec2,
    },
    CanvasZoom {
        mouse_ptr_timestamp: BigRational,
        delta: f32,
    },
    CursorSet(BigInt),
}

impl State {
    fn new(args: Args) -> Result<State> {
        let vcd_filename = args.vcd_file;

        println!("Loading vcd");
        let file = File::open(&vcd_filename)
                .with_context(|| format!("Failed to open {vcd_filename}"))?;

        let vcd = Some(
            parse_vcd(file)
                .map_err(|e| anyhow!("{e}"))
                .with_context(|| format!("Failed to parse parse {vcd_filename}"))?,
        );

        println!("Done loading vcd");

        let num_timestamps = vcd
            .as_ref()
            .and_then(|vcd| vcd.max_timestamp().as_ref().map(|t| t.to_bigint().unwrap()))
            .unwrap_or(BigInt::from_u32(1).unwrap());

        let translators = TranslatorList::new(vec![
            Box::new(translation::HexTranslator {}),
            Box::new(translation::UnsignedTranslator {}),
            Box::new(translation::HierarchyTranslator {}),
            // Box::new(PyTranslator::new("pytest", "translation_test.py").unwrap()),
        ]);

        Ok(State {
            active_scope: None,
            signals: vec![],
            control_key: false,
            viewport: Viewport::new(0., num_timestamps.clone().to_f64().unwrap()),
            num_timestamps,
            vcd,
            signal_format: HashMap::new(),
            translators,
            cursor: BigInt::from_u64(0).unwrap(),
        })
    }

    // TODO: Rename to process_msg or something
    fn update(&mut self, message: Message) {
        match message {
            Message::HierarchyClick(scope) => self.active_scope = Some(scope),
            Message::AddSignal(s) => {
                let translator = self.signal_translator(s);
                let info = translator.signal_info(&self.signal_name(s)).unwrap();
                self.signals.push((s, info))
            }
            Message::CanvasScroll { delta } => self.handle_canvas_scroll(delta),
            Message::CanvasZoom {
                delta,
                mouse_ptr_timestamp,
            } => self.handle_canvas_zoom(mouse_ptr_timestamp, delta as f64),
            Message::SignalFormatChange(idx, format) => {
                if self.translators.inner.contains_key(&format) {
                    *self.signal_format.entry(idx).or_default() = format;

                    let translator = self.signal_translator(idx);
                    let info = translator.signal_info(&self.signal_name(idx)).unwrap();
                    self.signals.retain(|(old_idx, _)| *old_idx != idx);
                    self.signals.push((idx, info))
                } else {
                    println!("WARN: No translator {format}")
                }
            }
            Message::CursorSet(new) => self.cursor = new,
        }
    }

    pub fn handle_canvas_scroll(
        &mut self,
        // Canvas relative
        delta: Vec2,
    ) {
        // Scroll 5% of the viewport per scroll event.
        // One scroll event yields 50
        let scroll_step = (&self.viewport.curr_right - &self.viewport.curr_left) / (50. * 20.);

        let target_left = &self.viewport.curr_left + scroll_step * delta.y as f64;
        let target_right = &self.viewport.curr_right + scroll_step * delta.y as f64;

        self.viewport.curr_left = target_left;
        self.viewport.curr_right = target_right;
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

        // TODO: Do not just round here, this will not work
        // for small zoom levels
        self.viewport.curr_left = target_left;
        self.viewport.curr_right = target_right;
    }

    pub fn signal_translator(&self, sig: SignalIdx) -> &Box<dyn Translator> {
        let translator_name = self
            .signal_format
            .get(&sig)
            .unwrap_or_else(|| &self.translators.default);
        let translator = &self.translators.inner[translator_name];
        translator
    }

    pub fn signal_name(&self, idx: SignalIdx) -> String {
        self.vcd
            .as_ref()
            .expect("Getting signal name from idx without vcd set")
            .signal_from_signal_idx(idx)
            .name()
    }
}
