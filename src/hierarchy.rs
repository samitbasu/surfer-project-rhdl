use eframe::egui::{Align, Layout, Margin, ScrollArea};

use crate::State;
use crate::{egui, message::Message};

pub fn separate(state: &mut State, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
    ui.visuals_mut().override_text_color = Some(state.config.theme.primary_ui_color.foreground);

    ui.with_layout(
        Layout::top_down(Align::LEFT).with_cross_justify(true),
        |ui| {
            let total_space = ui.available_height();
            egui::Frame::none()
                .inner_margin(Margin::same(5.0))
                .show(ui, |ui| {
                    ui.set_max_height(total_space / 2.);
                    ui.set_min_height(total_space / 2.);

                    ui.heading("Scopes");
                    ui.add_space(3.0);

                    ScrollArea::both().id_source("scopes").show(ui, |ui| {
                        ui.style_mut().wrap = Some(false);
                        if let Some(waves) = &state.waves {
                            state.draw_all_scopes(msgs, waves, false, ui, "");
                        }
                    });
                });

            egui::Frame::none()
                .inner_margin(Margin::same(5.0))
                .show(ui, |ui| {
                    let filter = &mut *state.sys.variable_name_filter.borrow_mut();
                    ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
                        ui.heading("Variables");
                        ui.add_space(3.0);
                        state.draw_variable_name_filter_edit(ui, filter, msgs);
                    });
                    ui.add_space(3.0);

                    ScrollArea::both()
                        .max_height(f32::INFINITY)
                        .id_source("variables")
                        .show(ui, |ui| {
                            if let Some(waves) = &state.waves {
                                if let Some(active_scope) = waves.active_scope.as_ref() {
                                    state.draw_variable_list(msgs, waves, ui, active_scope, filter);
                                }
                            }
                        });
                });
        },
    );
}

pub fn tree(state: &mut State, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
    ui.visuals_mut().override_text_color = Some(state.config.theme.primary_ui_color.foreground);

    ui.with_layout(
        Layout::top_down(Align::LEFT).with_cross_justify(true),
        |ui| {
            let total_space = ui.available_height();
            egui::Frame::none()
                .inner_margin(Margin::same(5.0))
                .show(ui, |ui| {
                    ui.set_max_height(total_space / 2.);
                    ui.set_min_height(total_space / 2.);

                    let filter = &mut *state.sys.variable_name_filter.borrow_mut();
                    ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
                        ui.heading("Hierarchy");
                        ui.add_space(3.0);
                        state.draw_variable_name_filter_edit(ui, filter, msgs);
                    });
                    ui.add_space(3.0);

                    ScrollArea::both().id_source("hierarchy").show(ui, |ui| {
                        ui.style_mut().wrap = Some(false);
                        if let Some(waves) = &state.waves {
                            state.draw_all_scopes(msgs, waves, true, ui, filter);
                        }
                    });
                });
        },
    );
}
