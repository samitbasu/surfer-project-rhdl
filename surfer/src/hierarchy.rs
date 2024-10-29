//! Functions for drawing the left hand panel showing scopes and variables.
use crate::message::Message;
use crate::transaction_container::StreamScopeRef;
use crate::wave_container::{ScopeRef, ScopeRefExt};
use crate::wave_data::ScopeType;
use crate::State;
use egui::{Frame, Layout, Margin, ScrollArea, TextWrapMode, Ui};
use emath::Align;
use itertools::Itertools;
use surfer_translation_types::VariableType;

/// Scopes and variables in two separate lists
pub fn separate(state: &mut State, ui: &mut Ui, msgs: &mut Vec<Message>) {
    ui.visuals_mut().override_text_color = Some(state.config.theme.primary_ui_color.foreground);

    ui.with_layout(
        Layout::top_down(Align::LEFT).with_cross_justify(true),
        |ui| {
            let total_space = ui.available_height();
            Frame::none()
                .inner_margin(Margin::same(5.0))
                .show(ui, |ui| {
                    ui.set_max_height(total_space / 2.);
                    ui.set_min_height(total_space / 2.);

                    ui.heading("Scopes");
                    ui.add_space(3.0);

                    ScrollArea::both().id_salt("scopes").show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        if let Some(waves) = &state.waves {
                            state.draw_all_scopes(msgs, waves, false, ui, "");
                        }
                    });
                });

            Frame::none()
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
                        .id_salt("variables")
                        .show(ui, |ui| {
                            if let Some(waves) = &state.waves {
                                let empty_scope = if waves.inner.is_waves() {
                                    ScopeType::WaveScope(ScopeRef::empty())
                                } else {
                                    ScopeType::StreamScope(StreamScopeRef::Empty(String::default()))
                                };
                                let active_scope =
                                    waves.active_scope.as_ref().unwrap_or(&empty_scope);
                                match active_scope {
                                    ScopeType::WaveScope(scope) => {
                                        let wave_container = waves.inner.as_waves().unwrap();
                                        let all_variables =
                                            wave_container.variables_in_scope(scope);
                                        if !state.show_parameters_in_scopes() {
                                            let parameters = all_variables
                                                .iter()
                                                .filter(|var| {
                                                    let meta =
                                                        wave_container.variable_meta(var).ok();
                                                    meta.unwrap().variable_type
                                                        == Some(VariableType::VCDParameter)
                                                })
                                                .cloned()
                                                .collect_vec();
                                            if !parameters.is_empty() {
                                                egui::collapsing_header::CollapsingState::load_with_default_open(
                                                    ui.ctx(),
                                                    egui::Id::new(&parameters),
                                                    false,
                                                )
                                                .show_header(ui, |ui| {
                                                    ui.with_layout(
                                                        Layout::top_down(Align::LEFT).with_cross_justify(true),
                                                        |ui| {
                                                            ui.label("Parameters");
                                                        },
                                                    );
                                                })
                                                .body(|ui| {
                                                    state.draw_variable_list(msgs, wave_container, ui, &parameters, filter);
                                                });
                                            }
                                        }
                                        let variables = all_variables
                                            .iter()
                                            .filter(|var| {
                                                let meta = wave_container.variable_meta(var).ok();
                                                meta.unwrap().variable_type
                                                    != Some(VariableType::VCDParameter)
                                            })
                                            .cloned()
                                            .collect_vec();
                                        state.draw_variable_list(
                                            msgs,
                                            wave_container,
                                            ui,
                                            &variables,
                                            filter,
                                        );
                                    }
                                    ScopeType::StreamScope(s) => {
                                        state.draw_transaction_variable_list(msgs, waves, ui, s);
                                    }
                                }
                            }
                        });
                });
        },
    );
}

/// Scopes and variables in a joint tree.
pub fn tree(state: &mut State, ui: &mut Ui, msgs: &mut Vec<Message>) {
    ui.visuals_mut().override_text_color = Some(state.config.theme.primary_ui_color.foreground);

    ui.with_layout(
        Layout::top_down(Align::LEFT).with_cross_justify(true),
        |ui| {
            Frame::none()
                .inner_margin(Margin::same(5.0))
                .show(ui, |ui| {
                    let filter = &mut *state.sys.variable_name_filter.borrow_mut();
                    ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
                        ui.heading("Hierarchy");
                        ui.add_space(3.0);
                        state.draw_variable_name_filter_edit(ui, filter, msgs);
                    });
                    ui.add_space(3.0);

                    ScrollArea::both().id_salt("hierarchy").show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        if let Some(waves) = &state.waves {
                            state.draw_all_scopes(msgs, waves, true, ui, filter);
                        }
                    });
                });
        },
    );
}
