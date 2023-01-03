use fastwave_backend::ScopeIdx;
use fastwave_backend::VCD;
use iced::widget::horizontal_space;
use iced::widget::pane_grid;
use iced::widget::pane_grid::Configuration;
use iced::widget::pick_list;
use iced::widget::scrollable;
use iced::widget::Canvas;
use iced::widget::Column;
use iced::widget::PaneGrid;
use iced::widget::{button, column, container, row, text};
use iced::Alignment;
use iced::Element;
use iced::Length;

use crate::{Message, State};

pub enum PanePurpose {
    Hierarchy,
    VarSelection,
    SignalList,
    SignalView,
}

pub fn pane_config() -> Configuration<PanePurpose> {
    Configuration::Split {
        axis: pane_grid::Axis::Vertical,
        ratio: 0.2,
        a: Box::new(Configuration::Split {
            axis: pane_grid::Axis::Horizontal,
            ratio: 0.5,
            a: Box::new(Configuration::Pane(PanePurpose::Hierarchy)),
            b: Box::new(Configuration::Pane(PanePurpose::VarSelection)),
        }),
        b: Box::new(Configuration::Split {
            axis: pane_grid::Axis::Vertical,
            ratio: 0.2,
            a: Box::new(Configuration::Pane(PanePurpose::SignalList)),
            b: Box::new(Configuration::Pane(PanePurpose::SignalView)),
        }),
    }
}

impl State {
    pub fn do_view(&self) -> Vec<Message> {
        let content: Element<Message> = if let Some(vcd) = self.vcd.as_ref() {
            let pane_grid = PaneGrid::new(&self.pane_state, move |_id, purpose, _is_maximized| {
                let content: Element<_> = match purpose {
                    PanePurpose::Hierarchy => {
                        let scopes = expand_scopes(vcd, &vcd.root_scopes_by_idx());
                        container(scrollable(scopes))
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .into()
                    }
                    PanePurpose::VarSelection => {
                        let vars = self.var_view(vcd);
                        container(
                            scrollable(vars)
                                .height(Length::Fill)
                                .on_scroll(|offset| Message::VarsScrolled(offset)),
                        )
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into()
                    }
                    PanePurpose::SignalList => {
                        let signal_list = self
                            .signals
                            .iter()
                            .map(|idx| {
                                pick_list(
                                    self.translators.names(),
                                    Some(vcd.signal_from_signal_idx(*idx).name()),
                                    |selected| Message::SignalFormatChange(idx.clone(), selected),
                                )
                                .width(Length::Fill)
                                .into()
                            })
                            .collect::<Vec<_>>();

                        Column::with_children(signal_list).into()
                    }
                    PanePurpose::SignalView => Canvas::new(self)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into(),
                };

                pane_grid::Content::new(content).style(if false {
                    style::pane_active
                } else {
                    style::pane_focused
                })
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .spacing(5)
            .on_resize(2, Message::GridResize);

            container(pane_grid)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            text(format!("no vcd loaded")).size(14).into()
        };

        container(row!(content))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn var_view(&self, vcd: &VCD) -> Element<Message> {
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
        vars.into()
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

mod style {
    use iced::{widget::container, Theme};

    pub fn pane_active(theme: &Theme) -> container::Appearance {
        let palette = theme.extended_palette();

        container::Appearance {
            background: Some(palette.background.weak.color.into()),
            border_width: 2.0,
            border_color: palette.background.strong.color,
            ..Default::default()
        }
    }

    pub fn pane_focused(theme: &Theme) -> container::Appearance {
        let palette = theme.extended_palette();

        container::Appearance {
            background: Some(palette.background.weak.color.into()),
            border_width: 2.0,
            border_color: palette.primary.strong.color,
            ..Default::default()
        }
    }
}
