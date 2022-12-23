mod signal_canvas;
mod view;

use fastwave_backend::parse_vcd;
use fastwave_backend::ScopeIdx;
use fastwave_backend::SignalIdx;
use iced::executor;
use iced::theme::Theme;
use iced::{Application, Command, Element, Settings, Subscription};

use fastwave_backend::VCD;

use std::fs::File;

pub fn main() -> iced::Result {
    State::run(Settings::default())
}

struct State {
    vcd: Option<VCD>,
    active_scope: Option<ScopeIdx>,
    signals: Vec<SignalIdx>,
}

#[derive(Debug, Clone)]
enum Message {
    HierarchyClick(ScopeIdx),
    VarsScrolled(f32),
    AddSignal(SignalIdx),
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
        (
            State {
                vcd: Some(parse_vcd(file).expect("Failed to parse vcd")),
                active_scope: None,
                signals: vec![],
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
                println!("adding signal");
                self.signals.push(s)
            }
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::none()
    }

    fn view(&self) -> Element<Message> {
        self.do_view()
    }
}
