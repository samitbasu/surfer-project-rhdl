use std::collections::HashMap;

use eframe::egui::{self, Align, Layout};
use fastwave_backend::{SignalIdx, VCD};
use pyo3::{exceptions::PyKeyboardInterrupt, PyResult, Python};

use crate::{translation::SignalInfo, Message, State};

/// Index used to keep track of traces and their sub-traces
pub(crate) type TraceIdx = (SignalIdx, Vec<String>);

impl eframe::App for State {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let max_width = ctx.available_rect().width();

        let mut msgs = vec![];

        egui::SidePanel::left("signal select left panel")
            .default_width(300.)
            .width_range(100.0..=max_width)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
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
                })
            });

        if let Some(vcd) = &self.vcd {
            let signal_offsets = egui::SidePanel::left("signal list")
                .default_width(300.)
                .width_range(100.0..=max_width)
                .show(ctx, |ui| {
                    ui.style_mut().wrap = Some(false);
                    ui.vertical(|ui| self.draw_var_list(&mut msgs, &vcd, ui))
                        .inner
                })
                .inner;

            egui::CentralPanel::default().show(ctx, |ui| {
                self.draw_signals(&mut msgs, &signal_offsets, vcd, ui);
            });
        };

        self.control_key = ctx.input().modifiers.ctrl;

        self.handle_ctrlc(ctx, frame);

        while let Some(msg) = msgs.pop() {
            self.update(msg);
        }
    }
}

impl State {
    fn handle_ctrlc(&self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Always repaint even if we're in the background. This is needed in order
        // to handle ctrl+c correctly
        ctx.request_repaint();

        // Make ctrl-c work even if no python code is being executed
        Python::with_gil(|py| {
            let result: PyResult<()> = py.run("a=0", None, None);

            match result {
                Ok(_) => {}
                Err(error) if error.is_instance_of::<PyKeyboardInterrupt>(py) => {
                    frame.close();
                }
                Err(_) => println!("Python exception in keyboard interrupt loop"),
            };
        });
    }

    pub fn draw_all_scopes(&self, msgs: &mut Vec<Message>, vcd: &VCD, ui: &mut egui::Ui) {
        for idx in vcd.root_scopes_by_idx() {
            self.draw_selectable_child_or_orphan_scope(msgs, vcd, idx, ui);
        }
    }

    fn draw_selectable_child_or_orphan_scope(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VCD,
        scope_idx: fastwave_backend::ScopeIdx,
        ui: &mut egui::Ui,
    ) {
        let name = vcd.scope_name_by_idx(scope_idx);
        let fastwave_backend::ScopeIdx(idx) = scope_idx;
        if vcd.child_scopes_by_idx(scope_idx).is_empty() {
            ui.add(egui::SelectableLabel::new(
                self.active_scope == Some(scope_idx),
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
                            self.active_scope == Some(scope_idx),
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
        vcd: &VCD,
        root_idx: fastwave_backend::ScopeIdx,
        ui: &mut egui::Ui,
    ) {
        for child_scope_idx in vcd.child_scopes_by_idx(root_idx) {
            self.draw_selectable_child_or_orphan_scope(msgs, vcd, child_scope_idx, ui);
        }
    }

    fn draw_signal_list(&self, msgs: &mut Vec<Message>, vcd: &VCD, ui: &mut egui::Ui) {
        if let Some(idx) = self.active_scope {
            for sig in vcd.get_children_signal_idxs(idx) {
                ui.with_layout(
                    Layout::top_down(Align::LEFT).with_cross_justify(true),
                    |ui| {
                        ui.add(egui::SelectableLabel::new(
                            false,
                            vcd.signal_from_signal_idx(sig).name(),
                        ))
                        .clicked()
                        .then(|| msgs.push(Message::AddSignal(sig)))
                    },
                );
            }
        }
    }

    fn draw_var_list(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VCD,
        ui: &mut egui::Ui,
    ) -> HashMap<TraceIdx, f32> {
        let mut signal_offsets = HashMap::new();

        for (sig, info) in &self.signals {
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    let name = vcd.signal_from_signal_idx(*sig).name();
                    let ctx_menu = self
                        .translators
                        .names()
                        .iter()
                        .map(|t| (t.clone(), Message::SignalFormatChange(*sig, t.clone())))
                        .collect();

                    self.draw_var(
                        msgs,
                        &name,
                        &(*sig, vec![]),
                        &mut signal_offsets,
                        info,
                        ui,
                        ctx_menu,
                    );
                },
            );
        }

        signal_offsets
    }

    fn draw_var(
        &self,
        msgs: &mut Vec<Message>,
        name: &str,
        path: &(SignalIdx, Vec<String>),
        signal_offsets: &mut HashMap<TraceIdx, f32>,
        info: &SignalInfo,
        ui: &mut egui::Ui,
        context_menu: Vec<(String, Message)>,
    ) {
        let draw_label = |ui: &mut egui::Ui| {
            ui.selectable_label(false, name).context_menu(|ui| {
                for (name, msg) in context_menu {
                    ui.button(name).clicked().then(|| {
                        ui.close_menu();
                        msgs.push(msg);
                    });
                }
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
                        self.draw_var(msgs, name, &path, signal_offsets, info, ui, vec![]);
                    }
                });

                signal_offsets.insert(path.clone(), response.1.response.rect.top());
            }
            SignalInfo::Bits => {
                let label = draw_label(ui);
                signal_offsets.insert(path.clone(), label.rect.top());
            }
        }
    }
}
