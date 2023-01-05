use eframe::egui::{self, Align, Key, Layout};
use fastwave_backend::VCD;

use crate::{Message, State};

impl eframe::App for State {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        egui::SidePanel::left("signal list")
            .default_width(300.)
            .width_range(100.0..=max_width)
            .show(ctx, |ui| {
                ui.style_mut().wrap = Some(false);
                ui.vertical(|ui| {
                    if let Some(vcd) = &self.vcd {
                        self.draw_var_list(&mut msgs, &vcd, ui);
                    }
                })
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_signals(&mut msgs, ui);
        });

        self.control_key = ctx.input().modifiers.ctrl;

        while let Some(msg) = msgs.pop() {
            self.update(msg);
        }
    }
}

impl State {
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

    fn draw_var_list(&self, msgs: &mut Vec<Message>, vcd: &VCD, ui: &mut egui::Ui) {
        for sig in &self.signals {
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    ui.add(egui::SelectableLabel::new(
                        false,
                        vcd.signal_from_signal_idx(*sig).name(),
                    ))
                    .context_menu(|ui| {
                        for name in self.translators.names() {
                            ui.button(&name)
                                .clicked()
                                .then(|| {
                                    ui.close_menu();
                                    msgs.push(Message::SignalFormatChange(*sig, name.clone()))
                                });
                        }
                    });
                });
        }
    }
}
