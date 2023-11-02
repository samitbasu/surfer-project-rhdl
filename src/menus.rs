use color_eyre::eyre::WrapErr;
use eframe::egui::{self, menu};

use crate::{
    clock_highlighting::ClockHighlightType, displayed_item::DisplayedItem, files::OpenMode,
    message::Message, signal_filter::signal_filter_type_menu, signal_name_type::SignalNameType,
    time::timescale_menu, translation::TranslationPreference, wave_container::FieldRef, State,
};

impl State {
    pub fn draw_menu(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
        menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("Open file...").clicked() {
                    msgs.push(Message::OpenFileDialog(OpenMode::Open));
                    ui.close_menu();
                }
                if ui.button("Switch file...").clicked() {
                    msgs.push(Message::OpenFileDialog(OpenMode::Switch));
                    ui.close_menu()
                }
                if ui.button("Open URL...").clicked() {
                    msgs.push(Message::SetUrlEntryVisible(true));
                    ui.close_menu();
                }
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("Exit").clicked() {
                    msgs.push(Message::Exit);
                    ui.close_menu();
                }
            });
            ui.menu_button("View", |ui| {
                if ui
                    .add(egui::Button::new("Zoom in").shortcut_text("+"))
                    .clicked()
                {
                    msgs.push(Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 0.5,
                    });
                }
                if ui
                    .add(egui::Button::new("Zoom out").shortcut_text("-"))
                    .clicked()
                {
                    msgs.push(Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 2.0,
                    });
                }
                if ui.button("Zoom to fit").clicked() {
                    ui.close_menu();
                    msgs.push(Message::ZoomToFit);
                }
                ui.separator();
                if ui
                    .add(egui::Button::new("Scroll to start").shortcut_text("s"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::GoToStart);
                }
                if ui
                    .add(egui::Button::new("Scroll to end").shortcut_text("e"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::GoToEnd);
                }
                ui.separator();
                if ui
                    .add(egui::Button::new("Toggle side panel").shortcut_text("b"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::ToggleSidePanel);
                }
                if ui
                    .add(egui::Button::new("Toggle menu").shortcut_text("m"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::ToggleMenu);
                }
                #[cfg(not(target_arch = "wasm32"))]
                if ui
                    .add(egui::Button::new("Toggle full screen").shortcut_text("F11"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::ToggleFullscreen);
                }
            });
            ui.menu_button("Settings", |ui| {
                ui.menu_button("Clock highlighting", |ui| {
                    let highlight_types = vec![
                        ClockHighlightType::Line,
                        ClockHighlightType::Cycle,
                        ClockHighlightType::None,
                    ];
                    for highlight_type in highlight_types {
                        ui.radio(
                            highlight_type == self.config.default_clock_highlight_type,
                            highlight_type.to_string(),
                        )
                        .clicked()
                        .then(|| {
                            ui.close_menu();
                            msgs.push(Message::SetClockHighlightType(highlight_type));
                        });
                    }
                });
                ui.menu_button("Time scale", |ui| {
                    timescale_menu(ui, msgs, &self.wanted_timescale);
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
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Control keys").clicked() {
                    ui.close_menu();
                    msgs.push(Message::SetKeyHelpVisible(true));
                }
                if ui.button("Mouse gestures").clicked() {
                    ui.close_menu();
                    msgs.push(Message::SetGestureHelpVisible(true));
                }
                ui.separator();
                if ui.button("About").clicked() {
                    ui.close_menu();
                    msgs.push(Message::SetAboutVisible(true));
                }
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
        if let Some(path) = path {
            self.add_format_menu(path, msgs, ui);
        }

        let displayed_item = &self.waves.as_ref().unwrap().displayed_items[vidx];
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

        if let DisplayedItem::Signal(signal) = &self.waves.as_ref().unwrap().displayed_items[vidx] {
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
        let Some(waves) = self.waves.as_ref() else {
            return;
        };

        let mut available_translators = if path.field.is_empty() {
            self.translators
                .all_translator_names()
                .into_iter()
                .filter(|translator_name| {
                    let t = self.translators.get_translator(translator_name);

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
            self.translators.basic_translator_names()
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
