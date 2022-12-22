mod signal_canvas;

use camino::Utf8Path;
use camino::Utf8PathBuf;
use fastwave_backend::parse_vcd;
use fastwave_backend::ScopeIdx;
use fastwave_backend::SignalIdx;
use iced::alignment;
use iced::executor;
use iced::theme::PaneGrid;
use iced::theme::{self, Theme};
use iced::time;
use iced::widget::horizontal_space;
use iced::widget::scrollable;
use iced::widget::Canvas;
use iced::widget::Column;
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Application, Command, Element, Length, Settings, Subscription};

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
        let file = File::open("full.vcd").expect("failed to open vcd");
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
        let content: Element<Message> = if let Some(vcd) = self.vcd.as_ref() {
            let scopes = expand_scopes(vcd, &vcd.root_scopes_by_idx());
            let vars: Column<_> = if let Some(active_scope) = self.active_scope {
                let signals = vcd.get_children_signal_idxs(active_scope);

                let signal_texts = signals
                    .iter()
                    .map(|signal| {
                        let name = vcd.signal_from_signal_idx(*signal).name();

                        Element::from(button(text(name)).on_press(Message::AddSignal(*signal)))
                    })
                    .collect::<Vec<_>>();

                Column::with_children(signal_texts)
            } else {
                column!()
            };

            let hierarchy = container(scopes)
                .width(Length::Fill)
                .height(Length::FillPortion(2));
            let var_view = container(
                scrollable(vars)
                    .height(Length::Fill)
                    .on_scroll(|offset| Message::VarsScrolled(offset)),
            )
            .width(Length::Fill)
            .height(Length::FillPortion(2));

            let signal_selection = column!(hierarchy, var_view).width(Length::FillPortion(2));

            let signal_list = self
                .signals
                .iter()
                .map(|idx| Element::from(text(vcd.signal_from_signal_idx(*idx).name())))
                .collect::<Vec<_>>();

            let signal_view = row!(
                Column::with_children(signal_list)
                    .width(Length::FillPortion(2))
                    .height(Length::Fill),
                Canvas::new(self)
                    .width(Length::FillPortion(5))
                    .height(Length::Fill)
            )
            .width(Length::FillPortion(5));

            Element::from(row!(signal_selection, signal_view))
        } else {
            text(format!("no vcd loaded")).size(14).into()
        };

        container(row!(content))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn expand_scopes<'a, 'b>(vcd: &'a VCD, scopes: &'a [ScopeIdx]) -> Element<'b, Message> {
    let elems = scopes
        .iter()
        .map(|s| {
            let name = vcd.scope_name_by_idx(s.clone());

            let self_elem = button(text(name)).on_press(Message::HierarchyClick(s.clone()));
            let children = expand_scopes(vcd, &vcd.child_scopes_by_idx(s.clone()));

            let child_container = container(
                row!(horizontal_space(10.into()), children).align_items(Alignment::Start),
            );

            container(
                column![self_elem, child_container]
                    .align_items(Alignment::Start)
                    .spacing(1),
            )
        })
        .collect::<Vec<_>>();

    container(Column::with_children(
        elems.into_iter().map(|c| c.into()).collect(),
    ))
    .into()
}
