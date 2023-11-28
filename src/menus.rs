use color_eyre::eyre::WrapErr;
use eframe::egui::{self, menu};

use crate::{
    clock_highlighting::clock_highlight_type_menu, displayed_item::DisplayedItem, message::Message,
    signal_filter::signal_filter_type_menu, signal_name_type::SignalNameType, time::timeunit_menu,
    translation::TranslationPreference, wave_container::FieldRef, wave_source::OpenMode, State,
};

// Button builder. Short name because we use it a ton
struct ButtonBuilder {
    text: String,
    shortcut: Option<String>,
    message: Message,
}

impl ButtonBuilder {
    fn new(text: impl Into<String>, message: Message) -> Self {
        Self {
            text: text.into(),
            message,
            shortcut: None,
        }
    }

    fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn add_leave_menu(self, msgs: &mut Vec<Message>, ui: &mut egui::Ui) {
        self.add_inner(false, msgs, ui)
    }
    pub fn add_closing_menu(self, msgs: &mut Vec<Message>, ui: &mut egui::Ui) {
        self.add_inner(true, msgs, ui)
    }

    pub fn add_inner(self, close_menu: bool, msgs: &mut Vec<Message>, ui: &mut egui::Ui) {
        let button = egui::Button::new(self.text);
        let button = if let Some(s) = self.shortcut {
            button.shortcut_text(s)
        } else {
            button
        };
        if ui.add(button).clicked() {
            msgs.push(self.message);
            if close_menu {
                ui.close_menu();
            }
        }
    }
}

impl State {
    pub fn draw_menu(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
        fn b(text: impl Into<String>, message: Message) -> ButtonBuilder {
            ButtonBuilder::new(text, message)
        }

        menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                b("Open file...", Message::OpenFileDialog(OpenMode::Open))
                    .add_closing_menu(msgs, ui);
                b("Switch file...", Message::OpenFileDialog(OpenMode::Switch))
                    .add_closing_menu(msgs, ui);
                b("Open URL...", Message::SetUrlEntryVisible(true)).add_closing_menu(msgs, ui);
                #[cfg(not(target_arch = "wasm32"))]
                b("Exit", Message::Exit).add_closing_menu(msgs, ui);
            });
            ui.menu_button("View", |ui| {
                b(
                    "Zoom in",
                    Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 0.5,
                    },
                )
                .shortcut("+")
                .add_leave_menu(msgs, ui);

                b(
                    "Zoom out",
                    Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 2.0,
                    },
                )
                .shortcut("-")
                .add_leave_menu(msgs, ui);

                b("Zoom to fit", Message::ZoomToFit).add_closing_menu(msgs, ui);

                ui.separator();

                b("Go to start", Message::GoToStart)
                    .shortcut("s")
                    .add_closing_menu(msgs, ui);
                b("Go to end", Message::GoToEnd)
                    .shortcut("e")
                    .add_closing_menu(msgs, ui);

                ui.separator();

                b("Toggle side panel", Message::ToggleSidePanel)
                    .shortcut("b")
                    .add_closing_menu(msgs, ui);
                b("Toggle menu", Message::ToggleSidePanel)
                    .shortcut("m")
                    .add_closing_menu(msgs, ui);
                b("Toggle full screen", Message::ToggleFullscreen)
                    .shortcut("F11")
                    .add_closing_menu(msgs, ui);
            });
            ui.menu_button("Settings", |ui| {
                ui.menu_button("Clock highlighting", |ui| {
                    clock_highlight_type_menu(ui, msgs, self.config.default_clock_highlight_type);
                });
                ui.menu_button("Time unit", |ui| {
                    timeunit_menu(ui, msgs, &self.wanted_timescale.unit);
                });
                if let Some(waves) = &self.waves {
                    let signal_name_type = waves.default_signal_name_type;
                    ui.menu_button("Signal names", |ui| {
                        let name_types = vec![
                            SignalNameType::Local,
                            SignalNameType::Global,
                            SignalNameType::Unique,
                        ];
                        for name_type in name_types {
                            ui.radio(signal_name_type == name_type, name_type.to_string())
                                .clicked()
                                .then(|| {
                                    ui.close_menu();
                                    msgs.push(Message::ForceSignalNameTypes(name_type));
                                });
                        }
                    });
                }
                ui.menu_button("Signal filter type", |ui| {
                    signal_filter_type_menu(ui, msgs, &self.signal_filter_type);
                });
                ui.menu_button("UI Scale", |ui| {
                    for scale in [0.5, 0.75, 1.0, 1.5, 2.0] {
                        ui.radio(self.ui_scale == Some(scale), format!("{} %", scale * 100.))
                            .clicked()
                            .then(|| {
                                ui.close_menu();
                                msgs.push(Message::SetUiScale(scale))
                            });
                    }
                })
            });
            ui.menu_button("Help", |ui| {
                b("Quick start", Message::SetQuickStartVisible(true)).add_closing_menu(msgs, ui);
                b("Control keys", Message::SetKeyHelpVisible(true)).add_closing_menu(msgs, ui);
                b("Mouse gestures", Message::SetGestureHelpVisible(true))
                    .add_closing_menu(msgs, ui);

                ui.separator();
                b("Show logs", Message::SetLogsVisible(true)).add_closing_menu(msgs, ui);

                ui.separator();

                b("About", Message::SetAboutVisible(true)).add_closing_menu(msgs, ui)
            });
        });
    }

    pub fn item_context_menu(
        &self,
        path: Option<&FieldRef>,
        msgs: &mut Vec<Message>,
        ui: &mut egui::Ui,
        vidx: usize,
    ) {
        let Some(waves) = &self.waves else { return };
        if let Some(path) = path {
            self.add_format_menu(path, msgs, ui);
        }

        let displayed_item = &waves.displayed_items[vidx];
        ui.menu_button("Color", |ui| {
            let selected_color = &displayed_item
                .color()
                .clone()
                .unwrap_or("__nocolor__".to_string());
            for color_name in self.config.theme.colors.keys() {
                ui.radio(selected_color == color_name, color_name)
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::ItemColorChange(
                            Some(vidx),
                            Some(color_name.clone()),
                        ));
                    });
            }
            ui.separator();
            ui.radio(selected_color == "__nocolor__", "Default")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ItemColorChange(Some(vidx), None));
                });
        });

        ui.menu_button("Background color", |ui| {
            let selected_color = &displayed_item
                .background_color()
                .clone()
                .unwrap_or("__nocolor__".to_string());
            for color_name in self.config.theme.colors.keys() {
                ui.radio(selected_color == color_name, color_name)
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::ItemBackgroundColorChange(
                            Some(vidx),
                            Some(color_name.clone()),
                        ));
                    });
            }
            ui.separator();
            ui.radio(selected_color == "__nocolor__", "Default")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ItemBackgroundColorChange(Some(vidx), None));
                });
        });

        if let DisplayedItem::Signal(signal) = displayed_item {
            ui.menu_button("Name", |ui| {
                let name_types = vec![
                    SignalNameType::Local,
                    SignalNameType::Global,
                    SignalNameType::Unique,
                ];
                let signal_name_type = signal.display_name_type;
                for name_type in name_types {
                    ui.radio(signal_name_type == name_type, name_type.to_string())
                        .clicked()
                        .then(|| {
                            ui.close_menu();
                            msgs.push(Message::ChangeSignalNameType(Some(vidx), name_type));
                        });
                }
            });
        }

        if ui.button("Remove").clicked() {
            msgs.push(Message::RemoveItem(vidx, 1));
            msgs.push(Message::InvalidateCount);
            ui.close_menu();
        }

        if path.is_none() {
            if ui.button("Rename").clicked() {
                ui.close_menu();
                msgs.push(Message::RenameItem(vidx));
            }
        }
    }

    fn add_format_menu(&self, path: &FieldRef, msgs: &mut Vec<Message>, ui: &mut egui::Ui) {
        // Should not call this unless a signal is selected, and, hence, a VCD is loaded
        let Some(waves) = &self.waves else { return };

        let mut available_translators = if path.field.is_empty() {
            self.sys
                .translators
                .all_translator_names()
                .into_iter()
                .filter(|translator_name| {
                    let t = self.sys.translators.get_translator(translator_name);

                    if self
                        .blacklisted_translators
                        .contains(&(path.root.clone(), (*translator_name).clone()))
                    {
                        false
                    } else {
                        match waves
                            .inner
                            .signal_meta(&path.root)
                            .and_then(|meta| t.translates(&meta))
                            .context(format!(
                                "Failed to check if {translator_name} translates {:?}",
                                path.root.full_path(),
                            )) {
                            Ok(TranslationPreference::Yes) => true,
                            Ok(TranslationPreference::Prefer) => true,
                            Ok(TranslationPreference::No) => false,
                            Err(e) => {
                                msgs.push(Message::BlacklistTranslator(
                                    path.root.clone(),
                                    (*translator_name).clone(),
                                ));
                                msgs.push(Message::Error(e));
                                false
                            }
                        }
                    }
                })
                .collect()
        } else {
            self.sys.translators.basic_translator_names()
        };

        available_translators.sort_by(|a, b| human_sort::compare(a, b));
        let format_menu = available_translators
            .iter()
            .map(|t| (*t, Message::SignalFormatChange(path.clone(), t.to_string())))
            .collect::<Vec<_>>();

        ui.menu_button("Format", |ui| {
            for (name, msg) in format_menu {
                ui.button(name).clicked().then(|| {
                    ui.close_menu();
                    msgs.push(msg);
                });
            }
        });
    }
}
