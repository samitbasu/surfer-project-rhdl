mod signal_canvas;
mod view;
mod viewport;

use fastwave_backend::parse_vcd;
use fastwave_backend::ScopeIdx;
use fastwave_backend::SignalIdx;
use iced::Font;
use iced::executor;
use iced::keyboard;
use iced::theme::Theme;
use iced::time;
use iced::widget::pane_grid;
use iced::widget::pane_grid::ResizeEvent;
use iced::{Application, Command, Element, Settings, Subscription};

use fastwave_backend::VCD;
use num::bigint::ToBigInt;
use num::BigInt;
use num::FromPrimitive;
use view::PanePurpose;
use viewport::Viewport;

use std::fs::File;
use std::time::Instant;

use crate::view::pane_config;

pub fn main() -> iced::Result {
    State::run(Settings::default())
}

struct State {
    vcd: Option<VCD>,
    active_scope: Option<ScopeIdx>,
    signals: Vec<SignalIdx>,
    /// The offset of the left side of the wave window in signal timestamps.
    viewport: Viewport,
    pane_state: pane_grid::State<PanePurpose>,
    control_key: bool,
    last_tick: Instant,
    num_timestamps: BigInt,
    font: Font,
}

#[derive(Debug, Clone)]
enum Message {
    HierarchyClick(ScopeIdx),
    VarsScrolled(f32),
    AddSignal(SignalIdx),
    GridResize(ResizeEvent),
    ControlKeyChange(bool),
    ChangeViewport(Viewport),
    Tick(Instant),
}

impl Application for State {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (State, Command<Message>) {
        println!("Loading vcd");
        let file = File::open("cpu.vcd").expect("failed to open vcd");
        println!("Done loading vcd");
        let panes = pane_grid::State::with_configuration(pane_config());

        let vcd = Some(parse_vcd(file).expect("Failed to parse vcd"));
        let num_timestamps = vcd
            .as_ref()
            .and_then(|vcd| vcd.max_timestamp().as_ref().map(|t| t.to_bigint().unwrap()))
            .unwrap_or(BigInt::from_u32(1).unwrap());

        let font = Font::External {
            name: "DejaVuSansMono",
            bytes: include_bytes!("/usr/share/fonts/TTF/DejaVuSansMono.ttf"),
        };

        (
            State {
                active_scope: None,
                signals: vec![],
                pane_state: panes,
                control_key: false,
                viewport: Viewport::new(BigInt::from_u32(0).unwrap(), num_timestamps.clone()),
                last_tick: Instant::now(),
                num_timestamps,
                vcd,
                font
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Stopwatch - Iced")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::HierarchyClick(scope) => self.active_scope = Some(scope),
            Message::VarsScrolled(_) => {}
            Message::AddSignal(s) => {
                self.signals.push(s)
            }
            Message::GridResize(r) => self.pane_state.resize(&r.split, r.ratio),
            Message::ControlKeyChange(val) => self.control_key = val,
            Message::ChangeViewport(new) => {
                self.viewport = new
            },
            Message::Tick(instant) => {
                self.viewport.interpolate(instant - self.last_tick);
                self.last_tick = instant;
            }
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let input = iced::subscription::events_with(|e, _| match e {
            iced::Event::Keyboard(k) => match k {
                keyboard::Event::ModifiersChanged(m) => {
                    Some(Message::ControlKeyChange(m.control()))
                }
                _ => None,
            },
            _ => None,
        });

        let time = time::every(time::Duration::from_millis(1)).map(Message::Tick);

        Subscription::batch(vec![input, time])
    }

    fn view(&self) -> Element<Message> {
        self.do_view()
    }
}
