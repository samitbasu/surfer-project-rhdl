use std::collections::HashMap;

use color_eyre::eyre::Context;
use eframe::egui::{self, style::Margin, Align, Color32, Event, Frame, Key, Layout, RichText};
use eframe::epaint::Vec2;
use fastwave_backend::SignalIdx;
use itertools::Itertools;
use log::trace;

use crate::util::uint_idx_to_alpha_idx;
use crate::{
    command_prompt::show_command_prompt,
    translation::{SignalInfo, TranslationPreference},
    Message, MoveDir, SignalDescriptor, State, VcdData,
};

/// Index used to keep track of traces and their sub-traces
pub(crate) type TraceIdx = (SignalIdx, Vec<String>);

impl eframe::App for State {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let max_width = ctx.available_rect().width();
        let max_height = ctx.available_rect().height();

        let mut msgs = vec![];

        if self.show_side_panel {
            egui::SidePanel::left("signal select left panel")
                .default_width(300.)
                .width_range(100.0..=max_width)
                .show(ctx, |ui| {
                    ui.with_layout(
                        Layout::top_down(Align::LEFT).with_cross_justify(true),
                        |ui| {
                            let total_space = ui.available_height();

                            egui::Frame::none().show(ui, |ui| {
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

                            egui::Frame::none().show(ui, |ui| {
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
            show_command_prompt(self, ctx, frame, &mut msgs);
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
                            |ui| self.draw_var_list(&mut msgs, &vcd, ui),
                        )
                        .inner
                    })
                    .inner;
                egui::CentralPanel::default()
                    .frame(Frame {
                        inner_margin: Margin::same(0.0),
                        outer_margin: Margin::same(0.0),
                        ..Default::default()
                    })
                    .show(ctx, |ui| {
                        self.draw_signals(&mut msgs, &signal_offsets, vcd, ui);
                    });
            } else {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(egui::Color32::BLACK))
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
                                |ui| {
                                    ui.label(
                                        RichText::new("🚀  Space:  Show command prompt")
                                            .monospace(),
                                    );
                                    ui.label(
                                        RichText::new(
                                            "〰  b    :  Show or hide the design hierarchy",
                                        )
                                        .monospace(),
                                    );
                                },
                            );
                        });
                    });
            }
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered_justified(|ui| {
                    let num_bytes = self
                        .vcd_progress
                        .1
                        .load(std::sync::atomic::Ordering::Relaxed);
                    if let Some(total) = self.vcd_progress.0 {
                        ui.monospace(format!("Loading. {num_bytes}/{total} kb loaded"));
                        let progress = num_bytes as f32 / total as f32;
                        let progress_bar = egui::ProgressBar::new(progress)
                            .show_percentage()
                            .desired_width(300.);

                        ui.add(progress_bar);
                    } else {
                        ui.monospace(format!("Loading. {num_bytes} bytes loaded"));
                    }
                });
            });
        };

        self.control_key = ctx.input().modifiers.ctrl;

        ctx.input().events.iter().for_each(|event| match event {
            Event::Key {
                key,
                pressed,
                modifiers: _,
            } => match (key, pressed, self.command_prompt.visible) {
                (Key::Space, true, false) => msgs.push(Message::ShowCommandPrompt(true)),
                (Key::Escape, true, true) => msgs.push(Message::ShowCommandPrompt(false)),
                (Key::B, true, false) => msgs.push(Message::ToggleSidePanel),
                (Key::J, true, false) => {
                    if self.control_key {
                        msgs.push(Message::MoveFocusedSignal(MoveDir::Down));
                    } else {
                        msgs.push(Message::MoveFocus(MoveDir::Down));
                    }
                }
                (Key::K, true, false) => {
                    if self.control_key {
                        msgs.push(Message::MoveFocusedSignal(MoveDir::Up));
                    } else {
                        msgs.push(Message::MoveFocus(MoveDir::Up));
                    }
                }
                (Key::ArrowDown, true, false) => {
                    if self.control_key {
                        msgs.push(Message::MoveFocusedSignal(MoveDir::Down));
                    } else {
                        msgs.push(Message::MoveFocus(MoveDir::Down));
                    }
                }
                (Key::ArrowUp, true, false) => {
                    if self.control_key {
                        msgs.push(Message::MoveFocusedSignal(MoveDir::Up));
                    } else {
                        msgs.push(Message::MoveFocus(MoveDir::Up));
                    }
                }
                (Key::Delete, true, false) => {
                    if let Some(idx) = self.focused_signal {
                        msgs.push(Message::RemoveSignal(idx));
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
        });

        self.handle_ctrlc(ctx, frame);

        loop {
            match self.msg_receiver.try_recv() {
                Ok(msg) => msgs.push(msg),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    trace!("Message sender disconnected");
                    break;
                }
            }
        }

        while let Some(msg) = msgs.pop() {
            self.update(msg);
        }
    }
}

impl State {
    fn handle_ctrlc(&self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Always repaint even if we're in the background. This is needed in order
        // to handle ctrl+c correctly
        ctx.request_repaint();

        // NOTE: This currently freezes the main thread when loading long running python
        // plugins, since those lock the gil

        // Make ctrl-c work even if no python code is being executed
        // Python::with_gil(|py| {
        //     let result: PyResult<()> = py.run("a=0", None, None);

        //     match result {
        //         Ok(_) => {}
        //         Err(error) if error.is_instance_of::<PyKeyboardInterrupt>(py) => {
        //             frame.close();
        //         }
        //         Err(_) => println!("Python exception in keyboard interrupt loop"),
        //     };
        // });
    }

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
            .then(|| msgs.push(Message::HierarchyClick(scope_idx)));
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
                        .then(|| msgs.push(Message::HierarchyClick(scope_idx)))
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
                .sorted_by_key(|(_, name)| name.clone());

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
    ) -> HashMap<TraceIdx, f32> {
        let mut signal_offsets = HashMap::new();

        for (vidx, (sig, info)) in vcd.signals.iter().enumerate() {
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    let name = vcd.inner.signal_from_signal_idx(*sig).name();

                    self.draw_var(
                        msgs,
                        vidx,
                        &name,
                        &(*sig, vec![]),
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
        signal_offsets: &mut HashMap<TraceIdx, f32>,
        info: &SignalInfo,
        ui: &mut egui::Ui,
    ) {
        let mut draw_label = |ui: &mut egui::Ui| {
            let tooltip = if let Some(vcd) = &self.vcd {
                if path.1.len() == 0 {
                    format!(
                        "Num bits: {}",
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
                            .background_color(Color32::GOLD)
                            .monospace()
                            .color(Color32::BLACK),
                    );
                }

                let label_bg_color = if self
                    .focused_signal
                    .map(|focused| focused == vidx)
                    .unwrap_or(false)
                {
                    Color32::DARK_RED
                } else {
                    Color32::TRANSPARENT
                };
                let signal_label = ui
                    .selectable_label(
                        false,
                        egui::RichText::new(name).background_color(label_bg_color),
                    )
                    .on_hover_text(tooltip)
                    .context_menu(|ui| {
                        let available_translators = if path.1.is_empty() {
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

                        let ctx_menu = available_translators
                            .iter()
                            .map(|t| (*t, Message::SignalFormatChange(path.clone(), t.to_string())))
                            .collect::<Vec<_>>();

                        ui.menu_button("Format", |ui| {
                            for (name, msg) in ctx_menu {
                                ui.button(name).clicked().then(|| {
                                    ui.close_menu();
                                    msgs.push(msg);
                                });
                            }
                        });

                        if ui.button("Remove").clicked() {
                            msgs.push(Message::RemoveSignal(vidx));
                            ui.close_menu();
                        }
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

                signal_offsets.insert(path.clone(), response.1.response.rect.top());
            }
            SignalInfo::Bool | SignalInfo::Bits => {
                let label = draw_label(ui);
                signal_offsets.insert(path.clone(), label.inner.rect.top());
            }
        }
    }
}
