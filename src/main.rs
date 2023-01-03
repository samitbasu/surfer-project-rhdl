// mod signal_canvas;
mod translation;
mod viewport;
mod view;

use eframe::egui;
use fastwave_backend::parse_vcd;
use fastwave_backend::ScopeIdx;
use fastwave_backend::SignalIdx;

use fastwave_backend::VCD;
use num::bigint::ToBigInt;
use num::BigInt;
use num::FromPrimitive;
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
        initial_window_size: Some(egui::vec2(320.0, 240.0)),
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
    ChangeViewport(Viewport),
    Tick(Instant),
    SignalFormatChange(SignalIdx, String),
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
            viewport: Viewport::new(BigInt::from_u32(0).unwrap(), num_timestamps.clone()),
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
            Message::ChangeViewport(new) => self.viewport = new,
            Message::Tick(instant) => {
                self.viewport.interpolate(instant - self.last_tick);
                self.last_tick = instant;
            }
            Message::SignalFormatChange(idx, format) => {
                if self.translators.inner.contains_key(&format) {
                    *self.signal_format.entry(idx).or_default() = format
                }
                else {
                    println!("WARN: No translator {format}")
                }
            }
        }
    }
}


