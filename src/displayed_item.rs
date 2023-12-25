use eframe::egui;
use log::warn;
use serde::{Deserialize, Serialize};

use crate::{
    message::Message, signal_name_type::SignalNameType, translation::SignalInfo,
    wave_container::SignalRef,
};

#[derive(Serialize, Deserialize)]
pub enum DisplayedItem {
    Signal(DisplayedSignal),
    Divider(DisplayedDivider),
    Cursor(DisplayedCursor),
    TimeLine(DisplayedTimeLine),
}

#[derive(Serialize, Deserialize)]
pub struct DisplayedSignal {
    pub signal_ref: SignalRef,
    #[serde(skip)]
    pub info: SignalInfo,
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub display_name: String,
    pub display_name_type: SignalNameType,
}

#[derive(Serialize, Deserialize)]
pub struct DisplayedDivider {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct DisplayedCursor {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub name: String,
    pub idx: u8,
}

#[derive(Serialize, Deserialize)]
pub struct DisplayedTimeLine {
    pub color: Option<String>,
    pub background_color: Option<String>,
}

impl DisplayedItem {
    pub fn color(&self) -> Option<String> {
        let color = match self {
            DisplayedItem::Signal(signal) => &signal.color,
            DisplayedItem::Divider(divider) => &divider.color,
            DisplayedItem::Cursor(cursor) => &cursor.color,
            DisplayedItem::TimeLine(timeline) => &timeline.color,
        };
        color.clone()
    }

    pub fn set_color(&mut self, color_name: Option<String>) {
        match self {
            DisplayedItem::Signal(signal) => {
                signal.color = color_name.clone();
            }
            DisplayedItem::Divider(divider) => {
                divider.color = color_name.clone();
            }
            DisplayedItem::Cursor(cursor) => {
                cursor.color = color_name.clone();
            }
            DisplayedItem::TimeLine(timeline) => {
                timeline.color = color_name.clone();
            }
        }
    }

    pub fn name(&self) -> String {
        match self {
            DisplayedItem::Signal(signal) => signal.display_name.clone(),
            DisplayedItem::Divider(divider) => divider.name.clone(),
            DisplayedItem::Cursor(cursor) => cursor.name.clone(),
            DisplayedItem::TimeLine(_) => "Time".to_string(),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            DisplayedItem::Signal(signal) => signal.display_name.clone(),
            DisplayedItem::Divider(divider) => divider.name.clone(),
            DisplayedItem::Cursor(cursor) => {
                format!("{idx}: {name}", idx = cursor.idx, name = cursor.name)
            }
            DisplayedItem::TimeLine(_) => "Time".to_string(),
        }
    }

    pub fn set_name(&mut self, name: String) {
        match self {
            DisplayedItem::Signal(_) => {
                warn!("Renaming signal");
            }
            DisplayedItem::Divider(divider) => {
                divider.name = name.clone();
            }
            DisplayedItem::Cursor(cursor) => {
                cursor.name = name.clone();
            }
            DisplayedItem::TimeLine(_) => {
                warn!("Renaming time line");
            }
        }
    }

    pub fn background_color(&self) -> Option<String> {
        let background_color = match self {
            DisplayedItem::Signal(signal) => &signal.background_color,
            DisplayedItem::Divider(divider) => &divider.background_color,
            DisplayedItem::Cursor(cursor) => &cursor.background_color,
            DisplayedItem::TimeLine(timeline) => &timeline.background_color,
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
        }
    }
}

pub fn draw_rename_window(
    ctx: &egui::Context,
    msgs: &mut Vec<Message>,
    idx: usize,
    name: &mut String,
) {
    let mut open = true;
    egui::Window::new("Rename item")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let response = ui.text_edit_singleline(name);
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    msgs.push(Message::ItemNameChange(Some(idx), name.clone()));
                    msgs.push(Message::SetRenameItemVisible(false));
                }
                response.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        msgs.push(Message::ItemNameChange(Some(idx), name.clone()));
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
