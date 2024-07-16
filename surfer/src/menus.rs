//! Menu handling.
use color_eyre::eyre::WrapErr;
use egui::{menu, Button, Context, TextWrapMode, TopBottomPanel, Ui};
use surfer_translation_types::{TranslationPreference, Translator};

use crate::wave_container::{FieldRef, VariableRefExt};
use crate::{
    clock_highlighting::clock_highlight_type_menu,
    config::{ArrowKeyBindings, HierarchyStyle},
    displayed_item::{DisplayedFieldRef, DisplayedItem, DisplayedItemIndex},
    message::Message,
    time::{timeformat_menu, timeunit_menu},
    variable_name_filter::variable_name_filter_type_menu,
    variable_name_type::VariableNameType,
    wave_source::OpenMode,
    State,
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

    pub fn add_leave_menu(self, msgs: &mut Vec<Message>, ui: &mut Ui) {
        self.add_inner(false, msgs, ui)
    }
    pub fn add_closing_menu(self, msgs: &mut Vec<Message>, ui: &mut Ui) {
        self.add_inner(true, msgs, ui)
    }

    pub fn add_inner(self, close_menu: bool, msgs: &mut Vec<Message>, ui: &mut Ui) {
        let button = Button::new(self.text);
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
    pub fn add_menu_panel(&self, ctx: &Context, msgs: &mut Vec<Message>) {
        TopBottomPanel::top("menu").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                self.menu_contents(ui, msgs);
            });
        });
    }

    pub fn menu_contents(&self, ui: &mut Ui, msgs: &mut Vec<Message>) {
        /// Helper function to get a new ButtonBuilder.
        fn b(text: impl Into<String>, message: Message) -> ButtonBuilder {
            ButtonBuilder::new(text, message)
        }

        let waves_loaded = self.waves.is_some();

        ui.menu_button("File", |ui| {
            b("Open file...", Message::OpenFileDialog(OpenMode::Open)).add_closing_menu(msgs, ui);
            b("Switch file...", Message::OpenFileDialog(OpenMode::Switch))
                .add_closing_menu(msgs, ui);
            b(
                "Reload",
                Message::ReloadWaveform(self.config.behavior.keep_during_reload),
            )
            .shortcut("r")
            .add_closing_menu(msgs, ui);

            b("Load state...", Message::LoadStateFile(None)).add_closing_menu(msgs, ui);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let save_text = if self.state_file.is_some() {
                    "Save state"
                } else {
                    "Save state..."
                };
                b(save_text, Message::SaveStateFile(self.state_file.clone()))
                    .add_closing_menu(msgs, ui);
            }
            b("Save state as...", Message::SaveStateFile(None)).add_closing_menu(msgs, ui);
            b("Open URL...", Message::SetUrlEntryVisible(true)).add_closing_menu(msgs, ui);
            #[cfg(not(target_arch = "wasm32"))]
            {
                b("Open Python translator", Message::OpenPythonPluginDialog)
                    .add_closing_menu(msgs, ui);
                // TODO: b("Reload Python translator", Message::ReloadPythonPlugin).add_closing_menu(msgs, ui);
                b("Exit", Message::Exit).add_closing_menu(msgs, ui);
            }
        });
        ui.menu_button("View", |ui: &mut Ui| {
            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
            if waves_loaded {
                b(
                    "Zoom in",
                    Message::CanvasZoom {
                        mouse_ptr: None,
                        delta: 0.5,
                        viewport_idx: 0,
                    },
                )
                .shortcut("+")
                .add_leave_menu(msgs, ui);

                b(
                    "Zoom out",
                    Message::CanvasZoom {
                        mouse_ptr: None,
                        delta: 2.0,
                        viewport_idx: 0,
                    },
                )
                .shortcut("-")
                .add_leave_menu(msgs, ui);

                b("Zoom to fit", Message::ZoomToFit { viewport_idx: 0 }).add_closing_menu(msgs, ui);

                ui.separator();

                b("Go to start", Message::GoToStart { viewport_idx: 0 })
                    .shortcut("s")
                    .add_closing_menu(msgs, ui);
                b("Go to end", Message::GoToEnd { viewport_idx: 0 })
                    .shortcut("e")
                    .add_closing_menu(msgs, ui);
                ui.separator();
                b("Add viewport", Message::AddViewport).add_closing_menu(msgs, ui);
                b("Remove viewport", Message::RemoveViewport).add_closing_menu(msgs, ui);
                ui.separator();
            }

            b("Toggle side panel", Message::ToggleSidePanel)
                .shortcut("b")
                .add_closing_menu(msgs, ui);
            b("Toggle menu", Message::ToggleMenu)
                .shortcut("m")
                .add_closing_menu(msgs, ui);
            b("Toggle toolbar", Message::ToggleToolbar)
                .shortcut("t")
                .add_closing_menu(msgs, ui);
            b("Toggle overview", Message::ToggleOverview).add_closing_menu(msgs, ui);
            b("Toggle statusbar", Message::ToggleStatusbar).add_closing_menu(msgs, ui);
            #[cfg(not(target_arch = "wasm32"))]
            b("Toggle full screen", Message::ToggleFullscreen)
                .shortcut("F11")
                .add_closing_menu(msgs, ui);
            ui.menu_button("Theme", |ui| {
                b("Default Theme", Message::SelectTheme(None)).add_closing_menu(msgs, ui);
                for theme_name in self.config.theme.theme_names.clone() {
                    b(theme_name.clone(), Message::SelectTheme(Some(theme_name)))
                        .add_closing_menu(msgs, ui);
                }
            });
        });

        ui.menu_button("Settings", |ui| {
            ui.menu_button("Clock highlighting", |ui| {
                clock_highlight_type_menu(ui, msgs, self.config.default_clock_highlight_type);
            });
            ui.menu_button("Time unit", |ui| {
                timeunit_menu(ui, msgs, &self.wanted_timeunit);
            });
            ui.menu_button("Time format", |ui| {
                timeformat_menu(ui, msgs, &self.get_time_format());
            });
            if let Some(waves) = &self.waves {
                let variable_name_type = waves.default_variable_name_type;
                ui.menu_button("Variable names", |ui| {
                    for name_type in enum_iterator::all::<VariableNameType>() {
                        ui.radio(variable_name_type == name_type, name_type.to_string())
                            .clicked()
                            .then(|| {
                                ui.close_menu();
                                msgs.push(Message::ForceVariableNameTypes(name_type));
                            });
                    }
                });
            }
            ui.menu_button("Variable name alignment", |ui| {
                let align_right = self
                    .align_names_right
                    .unwrap_or_else(|| self.config.layout.align_names_right());
                ui.radio(!align_right, "Left").clicked().then(|| {
                    ui.close_menu();
                    msgs.push(Message::SetNameAlignRight(false));
                });
                ui.radio(align_right, "Right").clicked().then(|| {
                    ui.close_menu();
                    msgs.push(Message::SetNameAlignRight(true));
                });
            });
            ui.menu_button("Variable filter type", |ui| {
                variable_name_filter_type_menu(ui, msgs, &self.variable_name_filter_type);
            });
            ui.menu_button("Zoom factor", |ui| {
                for scale in [0.5, 0.75, 1.0, 1.5, 2.0, 2.5] {
                    ui.radio(
                        self.ui_zoom_factor == Some(scale),
                        format!("{} %", scale * 100.),
                    )
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::SetUIZoomFactor(scale))
                    });
                }
            });

            ui.menu_button("Hierarchy", |ui| {
                for style in enum_iterator::all::<HierarchyStyle>() {
                    ui.radio(
                        self.config.layout.hierarchy_style == style,
                        style.to_string(),
                    )
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::SetHierarchyStyle(style));
                    });
                }
            });

            ui.menu_button("Arrow Keys", |ui| {
                for binding in enum_iterator::all::<ArrowKeyBindings>() {
                    ui.radio(
                        self.config.behavior.arrow_key_bindings == binding,
                        binding.to_string(),
                    )
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::SetArrowKeyBindings(binding));
                    });
                }
            });

            ui.radio(self.show_ticks(), "Show tick lines")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ToggleTickLines)
                });

            ui.radio(self.show_tooltip(), "Show variable tooltip")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ToggleVariableTooltip)
                });

            ui.radio(self.show_variable_indices(), "Show variable indices")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ToggleIndices)
                });
            ui.radio(self.show_variable_direction(), "Show variable direction")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ToggleDirection)
                });
        });
        ui.menu_button("Help", |ui| {
            b("Quick start", Message::SetQuickStartVisible(true)).add_closing_menu(msgs, ui);
            b("Control keys", Message::SetKeyHelpVisible(true)).add_closing_menu(msgs, ui);
            b("Mouse gestures", Message::SetGestureHelpVisible(true)).add_closing_menu(msgs, ui);

            ui.separator();
            b("Show logs", Message::SetLogsVisible(true)).add_closing_menu(msgs, ui);

            ui.separator();
            b("License information", Message::SetLicenseVisible(true)).add_closing_menu(msgs, ui);
            ui.separator();
            b("About", Message::SetAboutVisible(true)).add_closing_menu(msgs, ui);
        });
    }

    pub fn item_context_menu(
        &self,
        path: Option<&FieldRef>,
        msgs: &mut Vec<Message>,
        ui: &mut Ui,
        vidx: DisplayedItemIndex,
    ) {
        let Some(waves) = &self.waves else { return };

        let (displayed_item_id, displayed_item) = waves
            .displayed_items_order
            .get(vidx.0)
            .map(|id| (*id, &waves.displayed_items[id]))
            .unwrap();

        if let Some(path) = path {
            let dfr = DisplayedFieldRef {
                item: displayed_item_id,
                field: path.field.clone(),
            };
            self.add_format_menu(&dfr, displayed_item, path, msgs, ui);
        }

        ui.menu_button("Color", |ui| {
            let selected_color = &displayed_item
                .color()
                .clone()
                .unwrap_or_else(|| "__nocolor__".to_string());
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
                .unwrap_or_else(|| "__nocolor__".to_string());
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

        if let DisplayedItem::Variable(variable) = displayed_item {
            ui.menu_button("Name", |ui| {
                let variable_name_type = variable.display_name_type;
                for name_type in enum_iterator::all::<VariableNameType>() {
                    ui.radio(variable_name_type == name_type, name_type.to_string())
                        .clicked()
                        .then(|| {
                            ui.close_menu();
                            msgs.push(Message::ChangeVariableNameType(Some(vidx), name_type));
                        });
                }
            });
        }

        if ui.button("Rename").clicked() {
            ui.close_menu();
            msgs.push(Message::RenameItem(Some(vidx)));
        }

        if ui.button("Remove").clicked() {
            msgs.push(Message::RemoveItem(vidx, 1));
            msgs.push(Message::InvalidateCount);
            ui.close_menu();
        }
        if waves.cursor.is_some() && path.is_some() {
            ui.separator();
            if ui.button("Copy variable value").clicked() {
                ui.close_menu();
                msgs.push(Message::VariableValueToClipbord(Some(vidx)));
            }
        }

        ui.separator();
        ui.menu_button("Insert", |ui| {
            if ui.button("Divider").clicked() {
                ui.close_menu();
                msgs.push(Message::AddDivider(None, Some(vidx)));
            }
            if ui.button("Timeline").clicked() {
                ui.close_menu();
                msgs.push(Message::AddTimeLine(Some(vidx)));
            }
        });
    }

    fn add_format_menu(
        &self,
        displayed_field_ref: &DisplayedFieldRef,
        displayed_item: &DisplayedItem,
        path: &FieldRef,
        msgs: &mut Vec<Message>,
        ui: &mut Ui,
    ) {
        // Should not call this unless a variable is selected, and, hence, a VCD is loaded
        let Some(waves) = &self.waves else { return };

        let (mut preferred_translators, mut bad_translators) = if path.field.is_empty() {
            self.sys
                .translators
                .all_translator_names()
                .into_iter()
                .partition(|translator_name| {
                    let t = self.sys.translators.get_translator(translator_name);

                    if self
                        .blacklisted_translators
                        .contains(&(path.root.clone(), translator_name.to_string()))
                    {
                        false
                    } else {
                        match waves
                            .inner
                            .variable_meta(&path.root)
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
                                    translator_name.to_string(),
                                ));
                                msgs.push(Message::Error(e));
                                false
                            }
                        }
                    }
                })
        } else {
            (self.sys.translators.basic_translator_names(), vec![])
        };

        preferred_translators.sort_by(|a, b| numeric_sort::cmp(a, b));
        bad_translators.sort_by(|a, b| numeric_sort::cmp(a, b));

        let selected_translator = match displayed_item {
            DisplayedItem::Variable(var) => Some(var),
            _ => None,
        }
        .and_then(|displayed_variable| displayed_variable.get_format(&displayed_field_ref.field));

        let mut menu_entry = |ui: &mut Ui, name: &str| {
            ui.radio(
                selected_translator.map(|st| st == name).unwrap_or(false),
                name,
            )
            .clicked()
            .then(|| {
                ui.close_menu();
                msgs.push(Message::VariableFormatChange(
                    displayed_field_ref.clone(),
                    name.to_string(),
                ));
            });
        };

        ui.menu_button("Format", |ui| {
            for name in preferred_translators {
                menu_entry(ui, name);
            }
            if !bad_translators.is_empty() {
                ui.separator();
                ui.menu_button("Not recommended", |ui| {
                    for name in bad_translators {
                        menu_entry(ui, name);
                    }
                });
            }
        });
    }
}
