mod signal_canvas;
mod translation;
mod view;
mod viewport;

use eframe::egui;
use eframe::epaint::Pos2;
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
use translation::TranslatorList;
use viewport::Viewport;

use std::collections::HashMap;
use std::fs::File;
use std::time::Instant;

use crate::translation::pytranslator::PyTranslator;

enum Command {
    None,
    Loopback(Vec<Message>),
}

fn main() {
    let state = State::new();

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1920., 1080.)),
        ..Default::default()
    };
    eframe::run_native("My egui App", options, Box::new(|_cc| Box::new(state)))
}

struct State {
    vcd: Option<VCD>,
    active_scope: Option<ScopeIdx>,
    signals: Vec<SignalIdx>,
    /// The offset of the left side of the wave window in signal timestamps.
    viewport: Viewport,
    control_key: bool,
    last_tick: Instant,
    num_timestamps: BigInt,
    /// Which translator to use for each signal
    signal_format: HashMap<SignalIdx, String>,
    translators: TranslatorList,
}

#[derive(Debug, Clone)]
enum Message {
    HierarchyClick(ScopeIdx),
    VarsScrolled(f32),
    AddSignal(SignalIdx),
    ControlKeyChange(bool),
    Tick(Instant),
    SignalFormatChange(SignalIdx, String),
    CanvasScroll {
        delta: Vec2,
    },
    CanvasZoom {
        cursor_timestamp: BigRational,
        delta: f32,
    },
}

impl State {
    fn new() -> State {
        println!("Loading vcd");
        let file = File::open("cpu.vcd").expect("failed to open vcd");
        println!("Done loading vcd");

        let vcd = Some(parse_vcd(file).expect("Failed to parse vcd"));
        let num_timestamps = vcd
            .as_ref()
            .and_then(|vcd| vcd.max_timestamp().as_ref().map(|t| t.to_bigint().unwrap()))
            .unwrap_or(BigInt::from_u32(1).unwrap());

        let translators = TranslatorList::new(vec![
            Box::new(translation::HexTranslator {}),
            Box::new(translation::UnsignedTranslator {}),
            Box::new(PyTranslator::new("pytest", "translation_test.py").unwrap()),
        ]);

        State {
            active_scope: None,
            signals: vec![],
            control_key: false,
            viewport: Viewport::new(0., num_timestamps.clone().to_f64().unwrap()),
            last_tick: Instant::now(),
            num_timestamps,
            vcd,
            signal_format: HashMap::new(),
            translators,
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::HierarchyClick(scope) => self.active_scope = Some(scope),
            Message::VarsScrolled(_) => {}
            Message::AddSignal(s) => self.signals.push(s),
            Message::ControlKeyChange(val) => self.control_key = val,
            Message::CanvasScroll { delta } => self.handle_canvas_scroll(delta),
            Message::CanvasZoom {
                delta,
                cursor_timestamp,
            } => self.handle_canvas_zoom(cursor_timestamp, delta as f64),
            Message::Tick(instant) => {
                self.viewport.interpolate(instant - self.last_tick);
                self.last_tick = instant;
            }
            Message::SignalFormatChange(idx, format) => {
                if self.translators.inner.contains_key(&format) {
                    *self.signal_format.entry(idx).or_default() = format
                } else {
                    println!("WARN: No translator {format}")
                }
            }
        }
    }

    pub fn handle_canvas_scroll(
        &mut self,
        // Canvas relative
        delta: Vec2,
    ) {
        // Scroll 5% of the viewport per scroll event.
        // One scroll event yields 50
        let scroll_step = (&self.viewport.curr_right - &self.viewport.curr_left)
            / (50. * 20.);

        let target_left = &self.viewport.curr_left + scroll_step * delta.y as f64;
        let target_right = &self.viewport.curr_right + scroll_step * delta.y as f64;

        self.viewport.curr_left = target_left;
        self.viewport.curr_right = target_right;
    }

    pub fn handle_canvas_zoom(
        &mut self,
        // Canvas relative
        cursor_timestamp: BigRational,
        delta: f64,
    ) {
        // Zoom or scroll
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self.viewport;

        let target_left = (left - cursor_timestamp.to_f64().unwrap()) / delta
            + &cursor_timestamp.to_f64().unwrap();
        let target_right = (right - cursor_timestamp.to_f64().unwrap()) / delta
            + &cursor_timestamp.to_f64().unwrap();

        // TODO: Do not just round here, this will not work
        // for small zoom levels
        self.viewport.curr_left = target_left;
        self.viewport.curr_right = target_right;
    }
}
