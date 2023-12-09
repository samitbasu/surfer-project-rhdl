use eframe::egui::{Context, FontSelection, Key, RichText, Style, WidgetText, Window};
use eframe::emath::Align;
use eframe::epaint::{text::LayoutJob, Color32};
use serde::{Deserialize, Serialize};

use crate::{
    cursor::DEFAULT_CURSOR_NAME, message::Message, signal_name_type::SignalNameType,
    time::DEFAULT_TIMELINE_NAME, translation::SignalInfo, wave_container::SignalRef,
};

const DEFAULT_DIVIDER_NAME: &str = "";

#[derive(Serialize, Deserialize, Clone)]
pub enum DisplayedItem {
    Signal(DisplayedSignal),
    Divider(DisplayedDivider),
    Cursor(DisplayedCursor),
    TimeLine(DisplayedTimeLine),
    Placeholder(DisplayedPlaceholder),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedSignal {
    pub signal_ref: SignalRef,
    #[serde(skip)]
    pub info: SignalInfo,
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub display_name: String,
    pub display_name_type: SignalNameType,
    pub manual_name: Option<String>,
}

impl DisplayedSignal {
    pub fn to_placeholder(self) -> DisplayedPlaceholder {
        DisplayedPlaceholder {
            signal_ref: self.signal_ref,
            color: self.color,
            background_color: self.background_color,
            display_name: self.display_name,
            display_name_type: self.display_name_type,
            manual_name: self.manual_name,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedDivider {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedCursor {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub name: Option<String>,
    pub idx: u8,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedTimeLine {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedPlaceholder {
    pub signal_ref: SignalRef,
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub display_name: String,
    pub display_name_type: SignalNameType,
    pub manual_name: Option<String>,
}

impl DisplayedPlaceholder {
    pub fn to_signal(self, signal_info: SignalInfo) -> DisplayedSignal {
        DisplayedSignal {
            signal_ref: self.signal_ref,
            info: signal_info,
            color: self.color,
            background_color: self.background_color,
            display_name: self.display_name,
            display_name_type: self.display_name_type,
            manual_name: self.manual_name,
        }
    }
}

impl DisplayedItem {
    pub fn color(&self) -> Option<String> {
        match self {
            DisplayedItem::Signal(signal) => signal.color.clone(),
            DisplayedItem::Divider(divider) => divider.color.clone(),
            DisplayedItem::Cursor(cursor) => cursor.color.clone(),
            DisplayedItem::TimeLine(timeline) => timeline.color.clone(),
            DisplayedItem::Placeholder(_) => Some("grey".to_owned()), // TODO setting?
        } // TODO, single clone?
    }

    pub fn set_color(&mut self, color_name: Option<String>) {
        match self {
            DisplayedItem::Signal(signal) => signal.color = color_name.clone(),
            DisplayedItem::Divider(divider) => divider.color = color_name.clone(),
            DisplayedItem::Cursor(cursor) => cursor.color = color_name.clone(),
            DisplayedItem::TimeLine(timeline) => {
                timeline.color = color_name.clone();
            }
            DisplayedItem::Placeholder(placeholder) => placeholder.color = color_name.clone(),
        }
    }

    pub fn name(&self) -> String {
        match self {
            DisplayedItem::Signal(signal) => signal
                .manual_name
                .as_ref()
                .unwrap_or(&signal.display_name)
                .clone(),
            DisplayedItem::Divider(divider) => divider
                .name
                .as_ref()
                .unwrap_or(&DEFAULT_DIVIDER_NAME.to_string())
                .clone(),
            DisplayedItem::Cursor(cursor) => cursor
                .name
                .as_ref()
                .unwrap_or(&DEFAULT_CURSOR_NAME.to_string())
                .clone(),
            DisplayedItem::TimeLine(timeline) => timeline
                .name
                .as_ref()
                .unwrap_or(&DEFAULT_TIMELINE_NAME.to_string())
                .clone(),
            DisplayedItem::Placeholder(placeholder) => placeholder.display_name.clone(), // TODO investigate
        }
    }

    /// Widget displayed in signal list for the wave form, may include additional info compared to name()
    pub fn widget_text(&self, color: &Color32) -> WidgetText {
        match self {
            DisplayedItem::Signal(signal) => WidgetText::RichText(
                RichText::new(signal.manual_name.as_ref().unwrap_or(&signal.display_name))
                    .color(*color),
            ),
            DisplayedItem::Divider(divider) => WidgetText::RichText(
                RichText::new(
                    divider
                        .name
                        .as_ref()
                        .unwrap_or(&DEFAULT_DIVIDER_NAME.to_string()),
                )
                .color(*color)
                .italics(),
            ),
            DisplayedItem::Cursor(cursor) => {
                let style = Style::default();
                let mut layout_job = LayoutJob::default();
                RichText::new(format!("{idx}: ", idx = cursor.idx))
                    .color(*color)
                    .append_to(
                        &mut layout_job,
                        &style,
                        FontSelection::Default,
                        Align::Center,
                    );
                RichText::new(
                    cursor
                        .name
                        .as_ref()
                        .unwrap_or(&DEFAULT_CURSOR_NAME.to_string()),
                )
                .color(*color)
                .italics()
                .append_to(
                    &mut layout_job,
                    &style,
                    FontSelection::Default,
                    Align::Center,
                );
                WidgetText::LayoutJob(layout_job)
            }
            DisplayedItem::TimeLine(timeline) => WidgetText::RichText(
                RichText::new(
                    timeline
                        .name
                        .as_ref()
                        .unwrap_or(&DEFAULT_TIMELINE_NAME.to_string()),
                )
                .color(*color)
                .italics(),
            ),
            DisplayedItem::Placeholder(placeholder) => {
                let s = placeholder
                    .manual_name
                    .as_ref()
                    .unwrap_or(&placeholder.display_name);
                WidgetText::RichText(RichText::new("Not available: ".to_owned() + s))
                    .color(*color)
                    .italics()
            }
        }
    }

    pub fn set_name(&mut self, name: Option<String>) {
        match self {
            DisplayedItem::Signal(signal) => {
                signal.manual_name = name;
            }
            DisplayedItem::Divider(divider) => {
                divider.name = name;
            }
            DisplayedItem::Cursor(cursor) => {
                cursor.name = name;
            }
            DisplayedItem::TimeLine(timeline) => {
                timeline.name = name;
            }
            DisplayedItem::Placeholder(placeholder) => {
                placeholder.manual_name = name;
            }
        }
    }

    pub fn background_color(&self) -> Option<String> {
        let background_color = match self {
            DisplayedItem::Signal(signal) => &signal.background_color,
            DisplayedItem::Divider(divider) => &divider.background_color,
            DisplayedItem::Cursor(cursor) => &cursor.background_color,
            DisplayedItem::TimeLine(timeline) => &timeline.background_color,
            DisplayedItem::Placeholder(placeholder) => &placeholder.background_color,
        };
        background_color.clone()
    }

    pub fn set_background_color(&mut self, color_name: Option<String>) {
        match self {
            DisplayedItem::Signal(signal) => {
                signal.background_color = color_name.clone();
            }
            DisplayedItem::Divider(divider) => {
                divider.background_color = color_name.clone();
            }
            DisplayedItem::Cursor(cursor) => {
                cursor.background_color = color_name.clone();
            }
            DisplayedItem::TimeLine(timeline) => {
                timeline.background_color = color_name.clone();
            }
            DisplayedItem::Placeholder(placeholder) => {
                placeholder.background_color = color_name.clone();
            }
        }
    }
}

pub fn draw_rename_window(ctx: &Context, msgs: &mut Vec<Message>, idx: usize, name: &mut String) {
    let mut open = true;
    Window::new("Rename item")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let response = ui.text_edit_singleline(name);
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    msgs.push(Message::ItemNameChange(Some(idx), Some(name.clone())));
                    msgs.push(Message::SetRenameItemVisible(false));
                }
                response.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        msgs.push(Message::ItemNameChange(Some(idx), Some(name.clone())));
                        msgs.push(Message::SetRenameItemVisible(false));
                    }
                    if ui.button("Default").clicked() {
                        msgs.push(Message::ItemNameChange(Some(idx), None));
                        msgs.push(Message::SetRenameItemVisible(false));
                    }
                    if ui.button("Cancel").clicked() {
                        msgs.push(Message::SetRenameItemVisible(false));
                    }
                });
            });
        });
    if !open {
        msgs.push(Message::SetRenameItemVisible(false));
    }
}
