use color_eyre::eyre::Context;
use eframe::egui::{self, style::Margin, Align, Color32, Event, Key, Layout, RichText};
use eframe::egui::{menu, Frame, Grid, TextStyle};
use eframe::epaint::Vec2;
use fastwave_backend::{Metadata, SignalIdx, Timescale};
use itertools::Itertools;
use log::info;
use num::{BigInt, BigRational, ToPrimitive};
use spade_common::num_ext::InfallibleToBigInt;

use crate::descriptors::PathDescriptor;
use crate::util::uint_idx_to_alpha_idx;
use crate::{
    command_prompt::show_command_prompt,
    translation::{SignalInfo, TranslationPreference},
    Message, MoveDir, SignalDescriptor, State, VcdData,
};
use crate::{LoadProgress, SignalNameType};

/// Index used to keep track of traces and their sub-traces
pub(crate) type TraceIdx = (SignalIdx, Vec<String>);

pub struct SignalDrawingInfo {
    pub tidx: TraceIdx,
    pub signal_list_idx: usize,
    pub offset: f32,
}

impl eframe::App for State {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(not(target_arch = "wasm32"))]
        let window_size = Some(frame.info().window_info.size);
        #[cfg(target_arch = "wasm32")]
        let window_size = None;

        let mut msgs = self.draw(ctx, window_size);

        while let Some(msg) = msgs.pop() {
            #[cfg(not(target_arch = "wasm32"))]
            if let Message::Exit = msg {
                frame.close()
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Message::ToggleFullscreen = msg {
                frame.set_fullscreen(!frame.info().window_info.fullscreen)
            }
            self.update(msg);
        }

        self.handle_async_messages();
    }
}

impl State {
    pub(crate) fn draw(&self, ctx: &egui::Context, window_size: Option<Vec2>) -> Vec<Message> {
        let max_width = ctx.available_rect().width();
        let max_height = ctx.available_rect().height();

        let mut msgs = vec![];
        if self.config.layout.show_menu {
            egui::TopBottomPanel::top("menu").show(ctx, |ui| {
                self.draw_menu(ui, &mut msgs);
            });
        }
        if let Some(vcd) = &self.vcd {
            egui::TopBottomPanel::bottom("modeline")
                .frame(egui::containers::Frame {
                    fill: self.config.theme.primary_ui_color.background,
                    ..Default::default()
                })
                .show(ctx, |ui| {
                    ui.visuals_mut().override_text_color =
                        Some(self.config.theme.primary_ui_color.foreground);
                    ui.with_layout(Layout::left_to_right(Align::RIGHT), |ui| {
                        ui.add_space(10.0);
                        if self.show_wave_source {
                            ui.label(&vcd.filename);
                            if let Some(datetime) = vcd.inner.metadata.date {
                                ui.add_space(10.0);
                                ui.label(format!("Generated: {datetime}"));
                            }
                        }
                        if let Some(time) = &vcd.cursor {
                            ui.with_layout(Layout::right_to_left(Align::RIGHT), |ui| {
                                ui.label(time_string(
                                    time,
                                    &vcd.inner.metadata,
                                    &self.wanted_timescale,
                                ))
                                .context_menu(|ui| {
                                    timescale_menu(ui, &mut msgs, &self.wanted_timescale)
                                });
                                ui.add_space(10.0)
                            });
                        }
                    });
                });
        }

        if let Some(dialog) = &mut *self.file_dialog.borrow_mut() {
            if dialog.show(ctx).selected() {
                if let Some(file) = dialog.path() {
                    msgs.push(Message::LoadVcd(
                        camino::Utf8PathBuf::from_path_buf(file.to_path_buf()).expect("Unicode"),
                    ));
                }
            }
        }

        if self.config.layout.show_hierarchy {
            egui::SidePanel::left("signal select left panel")
                .default_width(300.)
                .width_range(100.0..=max_width)
                .frame(egui::containers::Frame {
                    fill: self.config.theme.primary_ui_color.background,
                    ..Default::default()
                })
                .show(ctx, |ui| {
                    ui.visuals_mut().override_text_color =
                        Some(self.config.theme.primary_ui_color.foreground);
                    ui.with_layout(
                        Layout::top_down(Align::LEFT).with_cross_justify(true),
                        |ui| {
                            let total_space = ui.available_height();
                            egui::Frame::none()
                                .inner_margin(Margin::same(5.0))
                                .show(ui, |ui| {
                                    ui.set_max_height(total_space / 2.);
                                    ui.set_min_height(total_space / 2.);

                                    ui.heading("Modules");
                                    ui.add_space(3.0);

                                    egui::ScrollArea::both()
                                        .id_source("modules")
                                        .show(ui, |ui| {
                                            ui.style_mut().wrap = Some(false);
                                            if let Some(vcd) = &self.vcd {
                                                self.draw_all_scopes(&mut msgs, vcd, ui);
                                            }
                                        });
                                });

                            egui::Frame::none()
                                .inner_margin(Margin::same(5.0))
                                .show(ui, |ui| {
                                    ui.heading("Signals");
                                    ui.add_space(3.0);

                                    egui::ScrollArea::both()
                                        .id_source("signals")
                                        .show(ui, |ui| {
                                            if let Some(vcd) = &self.vcd {
                                                self.draw_signal_list(&mut msgs, vcd, ui);
                                            }
                                        });
                                });
                        },
                    )
                });
        }

        if self.command_prompt.visible {
            show_command_prompt(self, ctx, window_size, &mut msgs);
        }

        if let Some(vcd_progress_data) = &self.vcd_progress {
            egui::TopBottomPanel::top("progress panel").show(ctx, |ui| {
                ui.vertical_centered_justified(|ui| match vcd_progress_data {
                    LoadProgress::Downloading(url) => {
                        ui.spinner();
                        ui.monospace(format!("Downloading {url}"));
                    }
                    LoadProgress::Loading(total_bytes, bytes_done) => {
                        let num_bytes = bytes_done.load(std::sync::atomic::Ordering::Relaxed);

                        if let Some(total) = total_bytes {
                            ui.monospace(format!("Loading. {num_bytes}/{total} kb loaded"));
                            let progress = num_bytes as f32 / *total as f32;
                            let progress_bar = egui::ProgressBar::new(progress)
                                .show_percentage()
                                .desired_width(300.);

                            ui.add(progress_bar);
                        } else {
                            ui.spinner();
                            ui.monospace(format!("Loading. {num_bytes} bytes loaded"));
                        };
                    }
                });
            });
        }

        if let Some(vcd) = &self.vcd {
            if !vcd.signals.is_empty() {
                let signal_offsets = egui::SidePanel::left("signal list")
                    .default_width(300.)
                    .width_range(100.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap = Some(false);
                        ui.with_layout(
                            Layout::top_down(Align::LEFT).with_cross_justify(true),
                            |ui| {
                                egui::ScrollArea::horizontal()
                                    .show(ui, |ui| self.draw_var_list(&mut msgs, &vcd, ui))
                                    .inner
                            },
                        )
                        .inner
                    })
                    .inner;

                egui::SidePanel::left("signal values")
                    .default_width(300.)
                    .width_range(100.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap = Some(false);
                        ui.with_layout(
                            Layout::top_down(Align::LEFT).with_cross_justify(true),
                            |ui| {
                                egui::ScrollArea::horizontal().show(ui, |ui| {
                                    self.draw_var_values(&signal_offsets, vcd, ui, &mut msgs)
                                })
                            },
                        )
                    });

                egui::CentralPanel::default()
                    .frame(Frame {
                        inner_margin: Margin::same(0.0),
                        outer_margin: Margin::same(0.0),
                        ..Default::default()
                    })
                    .show(ctx, |ui| {
                        self.draw_signals(&mut msgs, &signal_offsets, ui);
                    });
            }
        };

        if self.vcd.is_none()
            || self
                .vcd
                .as_ref()
                .map_or(false, |vcd| vcd.signals.is_empty())
        {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(self.config.theme.canvas_colors.background))
                .show(ctx, |ui| {
                    ui.add_space(max_height * 0.3);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🏄 Surfer").monospace().size(24.));
                        ui.add_space(20.);
                        let layout = egui::Layout::top_down(egui::Align::LEFT);
                        ui.allocate_ui_with_layout(
                            Vec2 {
                                x: max_width * 0.35,
                                y: max_height * 0.5,
                            },
                            layout,
                            |ui| self.help_message(ui),
                        );
                    });
                });
        }

        if self.show_about {
            let mut open = true;
            egui::Window::new("About Surfer")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🏄 Surfer").monospace().size(24.));
                        ui.add_space(20.);
                        ui.label(format!("Version: {ver}", ver = env!("CARGO_PKG_VERSION")));
                        ui.label(format!(
                            "Exact version: {info}",
                            info = env!("VERGEN_GIT_DESCRIBE")
                        ));
                        ui.label(format!(
                            "Build date: {date}",
                            date = env!("VERGEN_BUILD_DATE")
                        ));
                        ui.hyperlink_to(" repository", "https://gitlab.com/surfer-project/surfer");
                        ui.add_space(10.);
                        if ui.button("Close").clicked() {
                            msgs.push(Message::SetAboutVisible(false))
                        }
                    });
                });
            if !open {
                msgs.push(Message::SetAboutVisible(false))
            }
        }

        if self.show_keys {
            let mut open = true;
            egui::Window::new("🖮 Surfer control")
                .collapsible(true)
                .resizable(true)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        let layout = egui::Layout::top_down(egui::Align::LEFT);
                        ui.allocate_ui_with_layout(
                            Vec2 {
                                x: max_width * 0.35,
                                y: max_height * 0.5,
                            },
                            layout,
                            |ui| self.key_listing(ui),
                        );
                        ui.add_space(10.);
                        if ui.button("Close").clicked() {
                            msgs.push(Message::SetKeyHelpVisible(false))
                        }
                    });
                });
            if !open {
                msgs.push(Message::SetKeyHelpVisible(false))
            }
        }

        if self.show_url_entry {
            let mut open = true;
            egui::Window::new("Load URL")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        let url = &mut *self.url.borrow_mut();
                        ui.text_edit_singleline(url);
                        ui.horizontal(|ui| {
                            if ui.button("Load URL").clicked() {
                                msgs.push(Message::LoadVcdFromUrl(url.clone()));
                                msgs.push(Message::SetUrlEntryVisible(false))
                            }
                            if ui.button("Cancel").clicked() {
                                msgs.push(Message::SetUrlEntryVisible(false))
                            }
                        });
                    });
                });
            if !open {
                msgs.push(Message::SetUrlEntryVisible(false))
            }
        }

        ctx.input(|i| {
            i.raw.dropped_files.iter().for_each(|file| {
                info!("Got dropped file");
                msgs.push(Message::FileDropped(file.clone()))
            })
        });

        let control_key = ctx.input(|i| i.modifiers.ctrl);

        ctx.input(|i| {
            i.events.iter().for_each(|event| match event {
                Event::Key {
                    key,
                    repeat: _,
                    pressed,
                    modifiers: _,
                } => match (key, pressed, self.command_prompt.visible) {
                    (Key::Space, true, false) => msgs.push(Message::ShowCommandPrompt(true)),
                    (Key::Escape, true, true) => msgs.push(Message::ShowCommandPrompt(false)),
                    (Key::B, true, false) => msgs.push(Message::ToggleSidePanel),
                    (Key::M, true, false) => msgs.push(Message::ToggleMenu),
                    (Key::F11, true, false) => msgs.push(Message::ToggleFullscreen),
                    (Key::S, true, false) => msgs.push(Message::ScrollToStart),
                    (Key::E, true, false) => msgs.push(Message::ScrollToEnd),
                    (Key::Minus, true, false) => msgs.push(Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 2.0,
                    }),
                    (Key::PlusEquals, true, false) => msgs.push(Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 0.5,
                    }),
                    (Key::J, true, false) => {
                        if control_key {
                            msgs.push(Message::MoveFocusedSignal(MoveDir::Down));
                        } else {
                            msgs.push(Message::MoveFocus(MoveDir::Down));
                        }
                    }
                    (Key::K, true, false) => {
                        if control_key {
                            msgs.push(Message::MoveFocusedSignal(MoveDir::Up));
                        } else {
                            msgs.push(Message::MoveFocus(MoveDir::Up));
                        }
                    }
                    (Key::ArrowDown, true, false) => {
                        if control_key {
                            msgs.push(Message::MoveFocusedSignal(MoveDir::Down));
                        } else {
                            msgs.push(Message::MoveFocus(MoveDir::Down));
                        }
                    }
                    (Key::ArrowUp, true, false) => {
                        if control_key {
                            msgs.push(Message::MoveFocusedSignal(MoveDir::Up));
                        } else {
                            msgs.push(Message::MoveFocus(MoveDir::Up));
                        }
                    }
                    (Key::Delete, true, false) => {
                        if let Some(vcd) = &self.vcd {
                            if let Some(idx) = vcd.focused_signal {
                                msgs.push(Message::RemoveSignal(idx));
                            }
                        }
                    }
                    // this should be a shortcut to focusing
                    // to make this functional we need to make the cursor of the prompt
                    // point to the end of the input
                    // (Key::F, true, false) => {
                    //     self.command_prompt.input = String::from("focus_signal ");
                    //     msgs.push(Message::ShowCommandPrompt(true));
                    // }
                    _ => {}
                },
                _ => {}
            })
        });

        msgs
    }
}

impl State {
    pub fn draw_all_scopes(&self, msgs: &mut Vec<Message>, vcd: &VcdData, ui: &mut egui::Ui) {
        for idx in vcd.inner.root_scopes_by_idx() {
            self.draw_selectable_child_or_orphan_scope(msgs, vcd, idx, ui);
        }
    }

    fn draw_selectable_child_or_orphan_scope(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VcdData,
        scope_idx: fastwave_backend::ScopeIdx,
        ui: &mut egui::Ui,
    ) {
        let name = vcd.inner.scope_name_by_idx(scope_idx);
        let fastwave_backend::ScopeIdx(idx) = scope_idx;
        if vcd.inner.child_scopes_by_idx(scope_idx).is_empty() {
            ui.add(egui::SelectableLabel::new(
                vcd.active_scope == Some(scope_idx),
                name,
            ))
            .clicked()
            .then(|| msgs.push(Message::SetActiveScope(scope_idx.into())));
        } else {
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                egui::Id::new(idx),
                false,
            )
            .show_header(ui, |ui| {
                ui.with_layout(
                    Layout::top_down(Align::LEFT).with_cross_justify(true),
                    |ui| {
                        ui.add(egui::SelectableLabel::new(
                            vcd.active_scope == Some(scope_idx),
                            name,
                        ))
                        .clicked()
                        .then(|| msgs.push(Message::SetActiveScope(scope_idx.into())))
                    },
                );
            })
            .body(|ui| self.draw_root_scope_view(msgs, vcd, scope_idx, ui));
        }
    }

    fn draw_root_scope_view(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VcdData,
        root_idx: fastwave_backend::ScopeIdx,
        ui: &mut egui::Ui,
    ) {
        for child_scope_idx in vcd.inner.child_scopes_by_idx(root_idx) {
            self.draw_selectable_child_or_orphan_scope(msgs, vcd, child_scope_idx, ui);
        }
    }

    fn draw_signal_list(&self, msgs: &mut Vec<Message>, vcd: &VcdData, ui: &mut egui::Ui) {
        if let Some(idx) = vcd.active_scope {
            let signals = vcd.inner.get_children_signal_idxs(idx);
            let listed = signals
                .iter()
                .filter_map(|sig| {
                    let name = vcd.inner.signal_from_signal_idx(*sig).name();
                    if !name.starts_with("_e_") {
                        Some((sig, name.clone()))
                    } else {
                        None
                    }
                })
                .sorted_by(|a, b| human_sort::compare(&a.1, &b.1));

            for (sig, name) in listed {
                ui.with_layout(
                    Layout::top_down(Align::LEFT).with_cross_justify(true),
                    |ui| {
                        ui.add(egui::SelectableLabel::new(false, name))
                            .clicked()
                            .then(|| msgs.push(Message::AddSignal(SignalDescriptor::Id(*sig))));
                    },
                );
            }
        }
    }

    fn draw_var_list(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VcdData,
        ui: &mut egui::Ui,
    ) -> Vec<SignalDrawingInfo> {
        let mut signal_offsets = Vec::new();

        for (vidx, displayed_signal) in vcd.signals.iter().enumerate() {
            let sig = displayed_signal.idx;
            let info = &displayed_signal.info;
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    let signal = vcd.inner.signal_from_signal_idx(sig);

                    self.draw_var(
                        msgs,
                        vidx,
                        &displayed_signal.display_name,
                        &(signal.real_idx(), vec![]),
                        &mut signal_offsets,
                        info,
                        ui,
                    );
                },
            );
        }

        signal_offsets
    }

    fn draw_var(
        &self,
        msgs: &mut Vec<Message>,
        vidx: usize,
        name: &str,
        path: &(SignalIdx, Vec<String>),
        signal_offsets: &mut Vec<SignalDrawingInfo>,
        info: &SignalInfo,
        ui: &mut egui::Ui,
    ) {
        let mut draw_label = |ui: &mut egui::Ui| {
            let tooltip = if let Some(vcd) = &self.vcd {
                if path.1.len() == 0 {
                    format!(
                        "{}\nNum bits: {}",
                        vcd.ids_to_fullnames.get(&path.0).unwrap_or(&"".to_string()),
                        vcd.inner
                            .signal_from_signal_idx(path.0)
                            .num_bits()
                            .map(|v| format!("{v}"))
                            .unwrap_or("unknown".to_string())
                    )
                } else {
                    "From translator".to_string()
                }
            } else {
                "No VCD loaded".to_string()
            };
            ui.horizontal_top(|ui| {
                if self.command_prompt.expanded.starts_with("focus") {
                    let alpha_id = uint_idx_to_alpha_idx(
                        vidx,
                        self.vcd.as_ref().map_or(0, |vcd| vcd.signals.len()),
                    );
                    ui.label(
                        egui::RichText::new(alpha_id)
                            .background_color(self.config.theme.accent_warn.background)
                            .monospace()
                            .color(self.config.theme.accent_warn.foreground),
                    );
                }

                let focus_marker_color = if self
                    .vcd
                    .as_ref()
                    .expect("Can't draw a signal without a loaded waveform.")
                    .focused_signal
                    .map(|focused| focused == vidx)
                    .unwrap_or(false)
                {
                    self.config.theme.accent_info.background
                } else {
                    Color32::TRANSPARENT
                };
                ui.colored_label(focus_marker_color, "♦");

                let signal_label = ui
                    .selectable_label(false, egui::RichText::new(name))
                    .on_hover_text(tooltip)
                    .context_menu(|ui| {
                        self.signal_context_menu(path, msgs, ui, vidx);
                    });
                if signal_label.clicked() {
                    msgs.push(Message::FocusSignal(vidx))
                }
                signal_label
            })
        };

        match info {
            SignalInfo::Compound { subfields } => {
                let response = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    egui::Id::new(&path),
                    false,
                )
                .show_header(ui, draw_label)
                .body(|ui| {
                    for (name, info) in subfields {
                        let mut new_path = path.clone();
                        new_path.1.push(name.clone());
                        self.draw_var(msgs, vidx, name, &new_path, signal_offsets, info, ui);
                    }
                });

                let offset = response.0.rect.top();
                signal_offsets.push(SignalDrawingInfo {
                    tidx: path.clone(),
                    signal_list_idx: vidx,
                    offset,
                });
            }
            SignalInfo::Bool
            | SignalInfo::Bits
            | SignalInfo::Clock
            | SignalInfo::String
            | SignalInfo::Real => {
                let label = draw_label(ui);
                signal_offsets.push(SignalDrawingInfo {
                    tidx: path.clone(),
                    signal_list_idx: vidx,
                    offset: label.inner.rect.top(),
                });
            }
        }
    }

    fn signal_context_menu(
        &self,
        path: &(SignalIdx, Vec<String>),
        msgs: &mut Vec<Message>,
        ui: &mut egui::Ui,
        vidx: usize,
    ) {
        let mut available_translators = if path.1.is_empty() {
            self.translators
                .all_translator_names()
                .into_iter()
                .filter(|translator_name| {
                    let t = self.translators.get_translator(translator_name);

                    if self
                        .blacklisted_translators
                        .contains(&(path.0, (*translator_name).clone()))
                    {
                        false
                    } else {
                        self.vcd
                            .as_ref()
                            .map(|vcd| {
                                let sig = vcd.inner.signal_from_signal_idx(path.0);

                                match t.translates(&sig).context(format!(
                                    "Failed to check if {translator_name} translates {:?}",
                                    sig.path(),
                                )) {
                                    Ok(TranslationPreference::Yes) => true,
                                    Ok(TranslationPreference::Prefer) => true,
                                    Ok(TranslationPreference::No) => false,
                                    Err(e) => {
                                        msgs.push(Message::BlacklistTranslator(
                                            path.0,
                                            (*translator_name).clone(),
                                        ));
                                        msgs.push(Message::Error(e));
                                        false
                                    }
                                }
                            })
                            .unwrap_or(false)
                    }
                })
                .collect()
        } else {
            self.translators.basic_translator_names()
        };

        available_translators.sort_by(|a, b| human_sort::compare(a, b));
        let format_menu = available_translators
            .iter()
            .map(|t| {
                (
                    *t,
                    Message::SignalFormatChange(PathDescriptor::from_traceidx(path), t.to_string()),
                )
            })
            .collect::<Vec<_>>();

        ui.menu_button("Format", |ui| {
            for (name, msg) in format_menu {
                ui.button(name).clicked().then(|| {
                    ui.close_menu();
                    msgs.push(msg);
                });
            }
        });

        ui.menu_button("Color", |ui| {
            for color_name in self.config.theme.colors.keys() {
                ui.button(color_name).clicked().then(|| {
                    ui.close_menu();
                    msgs.push(Message::SignalColorChange(Some(vidx), color_name.clone()));
                });
            }
        });

        ui.menu_button("Name", |ui| {
            let name_types = vec![
                SignalNameType::Local,
                SignalNameType::Global,
                SignalNameType::Unique,
            ];
            let signal_name_type = self
                .vcd
                .as_ref()
                .map(|vcd| vcd.signals[vidx].display_name_type)
                .unwrap();
            for name_type in name_types {
                ui.radio(signal_name_type == name_type, name_type.to_string())
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::ChangeSignalNameType(Some(vidx), name_type));
                    });
            }
        });

        if ui.button("Remove").clicked() {
            msgs.push(Message::RemoveSignal(vidx));
            ui.close_menu();
        }
    }

    fn draw_var_values(
        &self,
        signal_offsets: &[SignalDrawingInfo],
        vcd: &VcdData,
        ui: &mut egui::Ui,
        msgs: &mut Vec<Message>,
    ) {
        if let Some(cursor) = &vcd.cursor {
            let text_style = TextStyle::Monospace;
            ui.style_mut().override_text_style = Some(text_style);

            for (vidx, drawing_info) in signal_offsets
                .iter()
                .sorted_by_key(|o| o.offset as i32)
                .enumerate()
            {
                let next_y = ui.cursor().top();
                // In order to align the text in this view with the variable tree,
                // we need to keep track of how far away from the expected offset we are,
                // and compensate for it
                if next_y < drawing_info.offset {
                    ui.add_space(drawing_info.offset - next_y);
                }

                let translator =
                    vcd.signal_translator((drawing_info.tidx.0, vec![]), &self.translators);

                let signal = vcd.inner.signal_from_signal_idx(drawing_info.tidx.0);

                if cursor < &0.to_bigint() {
                    break;
                }

                let translation_result = signal
                    .query_val_on_tmln(&num::BigInt::to_biguint(&cursor).unwrap(), &vcd.inner)
                    .map(|(_time, value)| translator.translate(&signal, &value));

                if let Ok(Ok(s)) = translation_result {
                    let subfields = s
                        .flatten(
                            (drawing_info.tidx.0, vec![]),
                            &vcd.signal_format,
                            &self.translators,
                        )
                        .as_fields();

                    let subfield = subfields.iter().find(|(k, _)| k == &drawing_info.tidx.1);

                    if let Some((_, Some((v, _)))) = subfield {
                        ui.label(v).context_menu(|ui| {
                            self.signal_context_menu(&(signal.real_idx(), vec![]), msgs, ui, vidx);
                        });
                    } else {
                        ui.label("-");
                    }
                }
            }
        }
    }

    fn controls_listing(&self, ui: &mut egui::Ui) {
        let controls = vec![
            ("🚀", "Space", "Show command prompt"),
            ("↔", "Scroll", "Pan"),
            ("🔎", "Ctrl+Scroll", "Zoom"),
            ("〰", "b", "Show or hide the design hierarchy"),
            ("☰", "m", "Show or hide menu"),
        ];

        Grid::new("controls")
            .num_columns(2)
            .spacing([20., 5.])
            .show(ui, |ui| {
                for (symbol, control, description) in controls {
                    ui.label(format!("{symbol}  {control}"));
                    ui.label(description);
                    ui.end_row();
                }
            });
    }

    fn key_listing(&self, ui: &mut egui::Ui) {
        let keys = vec![
            ("🚀", "Space", "Show command prompt"),
            ("↔", "Scroll", "Pan"),
            ("🔎", "Ctrl+Scroll", "Zoom"),
            ("〰", "b", "Show or hide the design hierarchy"),
            ("☰", "m", "Show or hide menu"),
            ("🔎➕", "+", "Zoom in"),
            ("🔎➖", "-", "Zoom out"),
            ("", "k/⬆", "Move focus up"),
            ("", "j/⬇", "Move focus down"),
            ("", "Ctrl+k/⬆", "Move focused signal up"),
            ("", "Ctrl+j/⬇", "Move focused signal down"),
            ("🔙", "s", "Scroll to start"),
            ("🔚", "e", "Scroll to end"),
            ("🗙", "Delete", "Delete focused signal"),
            #[cfg(not(target_arch = "wasm32"))]
            ("⛶", "F11", "Toggle full screen"),
        ];

        Grid::new("keys")
            .num_columns(3)
            .spacing([5., 5.])
            .show(ui, |ui| {
                for (symbol, control, description) in keys {
                    ui.label(symbol);
                    ui.label(control);
                    ui.label(description);
                    ui.end_row();
                }
            });
    }

    fn help_message(&self, ui: &mut egui::Ui) {
        if self.vcd.is_none() {
            ui.label(RichText::new("Drag and drop a VCD file here to open it"));

            #[cfg(target_arch = "wasm32")]
            ui.label(RichText::new("Or press space and type load_url"));
            #[cfg(not(target_arch = "wasm32"))]
            ui.label(RichText::new(
                "Or press space and type load_vcd or load_url",
            ));
            #[cfg(target_arch = "wasm32")]
            ui.label(RichText::new("Or use the file menu to open a URL"));
            #[cfg(not(target_arch = "wasm32"))]
            ui.label(RichText::new(
                "Or use the file menu to open a file or a URL",
            ));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Or click"));
                if ui.link("here").clicked() {
                    self.msg_sender
                        .send(Message::LoadVcdFromUrl(
                            "https://app.surfer-project.org/picorv32.vcd".to_string(),
                        ))
                        .ok();
                }
                ui.label("to open an example waveform");
            });

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(20.0);
        }

        self.controls_listing(ui);

        ui.add_space(20.0);
        ui.separator();
        ui.add_space(20.0);
        if let Some(vcd) = &self.vcd {
            ui.label(RichText::new(format!("Filename: {}", vcd.filename)).monospace());
        }

        #[cfg(target_arch = "wasm32")]
        {
            ui.label(RichText::new(
                "Note that this web based version is a bit slower than a natively installed version. There may also be a long delay with unresponsiveness when loading large waveforms because the web assembly version does not currently support multi threading.",
            ));

            ui.hyperlink_to(
                "See https://gitlab.com/surfer-project/surfer for install instructions",
                "https://gitlab.com/surfer-project/surfer",
            );
        }
    }

    fn draw_menu(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
        menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("Open file...").clicked() {
                    msgs.push(Message::OpenFileDialog);
                    ui.close_menu();
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
                    msgs.push(Message::ScrollToStart);
                }
                if ui
                    .add(egui::Button::new("Scroll to end").shortcut_text("e"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::ScrollToEnd);
                }
                ui.separator();
                ui.menu_button("Signal names", |ui| {
                    let name_types = vec![
                        SignalNameType::Local,
                        SignalNameType::Global,
                        SignalNameType::Unique,
                    ];
                    for name_type in name_types {
                        ui.button(name_type.to_string()).clicked().then(|| {
                            ui.close_menu();
                            msgs.push(Message::ForceSignalNameTypes(name_type));
                        });
                    }
                });
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
                ui.separator();
                ui.menu_button("Time scale", |ui| {
                    timescale_menu(ui, msgs, &self.wanted_timescale);
                });
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Control keys").clicked() {
                    ui.close_menu();
                    msgs.push(Message::SetKeyHelpVisible(true));
                }
                ui.separator();
                if ui.button("About").clicked() {
                    ui.close_menu();
                    msgs.push(Message::SetAboutVisible(true));
                }
            });
        });
    }
}

fn timescale_menu(ui: &mut egui::Ui, msgs: &mut Vec<Message>, wanted_timescale: &Timescale) {
    let timescales = vec![
        Timescale::Fs,
        Timescale::Ps,
        Timescale::Ns,
        Timescale::Us,
        Timescale::Ms,
        Timescale::S,
    ];
    for timescale in timescales {
        ui.radio(*wanted_timescale == timescale, timescale.to_string())
            .clicked()
            .then(|| {
                ui.close_menu();
                msgs.push(Message::SetTimeScale(timescale));
            });
    }
}

pub fn time_string(time: &BigInt, metadata: &Metadata, wanted_timescale: &Timescale) -> String {
    let wanted_exponent = timescale_to_exponent(wanted_timescale);
    let data_exponent = timescale_to_exponent(&metadata.timescale.1);
    let exponent_diff = wanted_exponent - data_exponent;
    if exponent_diff >= 0 {
        let precision = exponent_diff as usize;
        format!(
            "{scaledtime:.precision$} {wanted_timescale}",
            scaledtime = BigRational::new(
                time * metadata.timescale.0.unwrap_or(1),
                (BigInt::from(10)).pow(exponent_diff as u32)
            )
            .to_f64()
            .unwrap_or(f64::NAN)
        )
    } else {
        format!(
            "{scaledtime} {wanted_timescale}",
            scaledtime = time
                * metadata.timescale.0.unwrap_or(1)
                * (BigInt::from(10)).pow(-exponent_diff as u32)
        )
    }
}

fn timescale_to_exponent(timescale: &Timescale) -> i8 {
    match timescale {
        Timescale::Fs => -15,
        Timescale::Ps => -12,
        Timescale::Ns => -9,
        Timescale::Us => -6,
        Timescale::Ms => -3,
        Timescale::S => 0,
        _ => 0,
    }
}
