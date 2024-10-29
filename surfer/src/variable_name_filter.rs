//! Filtering of the variable list.
use derive_more::Display;
use egui::{Button, Layout, TextEdit, Ui};
use egui_remixicon::icons;
use emath::{Align, Vec2};
use enum_iterator::Sequence;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use itertools::Itertools;
use regex::{escape, Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::data_container::DataContainer::Transactions;
use crate::transaction_container::{StreamScopeRef, TransactionStreamRef};
use crate::wave_data::ScopeType;
use crate::{message::Message, wave_container::VariableRef, State};

#[derive(Debug, Display, PartialEq, Serialize, Deserialize, Sequence)]
pub enum VariableNameFilterType {
    #[display("Fuzzy")]
    Fuzzy,

    #[display("Regular expression")]
    Regex,

    #[display("Variable starts with")]
    Start,

    #[display("Variable contains")]
    Contain,
}

impl VariableNameFilterType {
    pub fn matching_variables(
        &self,
        variables: &[VariableRef],
        filter: &str,
        case_insensitive: bool,
    ) -> Vec<VariableRef> {
        match self {
            VariableNameFilterType::Fuzzy => {
                let matcher = if case_insensitive {
                    SkimMatcherV2::default().ignore_case()
                } else {
                    SkimMatcherV2::default().respect_case()
                };
                variables
                    .iter()
                    .filter(|var| matcher.fuzzy_match(&var.name, filter).is_some())
                    .cloned()
                    .collect_vec()
            }
            VariableNameFilterType::Contain => {
                if case_insensitive {
                    if let Ok(regex) = RegexBuilder::new(&escape(filter))
                        .case_insensitive(true)
                        .build()
                    {
                        variables
                            .iter()
                            .filter(|var| regex.is_match(&var.name))
                            .cloned()
                            .collect_vec()
                    } else {
                        Vec::new()
                    }
                } else {
                    variables
                        .iter()
                        .filter(|var| var.name.contains(filter))
                        .cloned()
                        .collect_vec()
                }
            }
            VariableNameFilterType::Start => {
                if case_insensitive {
                    if let Ok(regex) = RegexBuilder::new(&format!("^{}", escape(filter)))
                        .case_insensitive(true)
                        .build()
                    {
                        variables
                            .iter()
                            .filter(|var| regex.is_match(&var.name))
                            .cloned()
                            .collect_vec()
                    } else {
                        Vec::new()
                    }
                } else {
                    variables
                        .iter()
                        .filter(|var| var.name.starts_with(filter))
                        .cloned()
                        .collect_vec()
                }
            }
            VariableNameFilterType::Regex => {
                if let Ok(regex) = RegexBuilder::new(filter)
                    .case_insensitive(case_insensitive)
                    .build()
                {
                    variables
                        .iter()
                        .filter(|var| regex.is_match(&var.name))
                        .cloned()
                        .collect_vec()
                } else {
                    Vec::new()
                }
            }
        }
    }
}

impl State {
    pub fn draw_variable_name_filter_edit(
        &self,
        ui: &mut Ui,
        filter: &mut String,
        msgs: &mut Vec<Message>,
    ) {
        ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
            let default_padding = ui.spacing().button_padding;
            ui.spacing_mut().button_padding = Vec2 {
                x: 0.,
                y: default_padding.y,
            };
            ui.button(icons::ADD_FILL)
                .on_hover_text("Add all variables from active Scope")
                .clicked()
                .then(|| {
                    if let Some(waves) = self.waves.as_ref() {
                        // Iterate over the reversed list to get
                        // waves in the same order as the variable
                        // list
                        if let Some(active_scope) = waves.active_scope.as_ref() {
                            match active_scope {
                                ScopeType::WaveScope(active_scope) => {
                                    let variables = waves
                                        .inner
                                        .as_waves()
                                        .unwrap()
                                        .variables_in_scope(active_scope);
                                    msgs.push(Message::AddVariables(
                                        self.filtered_variables(&variables, filter),
                                    ));
                                }
                                ScopeType::StreamScope(active_scope) => {
                                    let Transactions(inner) = &waves.inner else {
                                        return;
                                    };
                                    match active_scope {
                                        StreamScopeRef::Root => {
                                            for stream in inner.get_streams() {
                                                msgs.push(Message::AddStreamOrGenerator(
                                                    TransactionStreamRef::new_stream(
                                                        stream.id,
                                                        stream.name.clone(),
                                                    ),
                                                ));
                                            }
                                        }
                                        StreamScopeRef::Stream(s) => {
                                            for gen_id in
                                                &inner.get_stream(s.stream_id).unwrap().generators
                                            {
                                                let gen = inner.get_generator(*gen_id).unwrap();

                                                msgs.push(Message::AddStreamOrGenerator(
                                                    TransactionStreamRef::new_gen(
                                                        gen.stream_id,
                                                        gen.id,
                                                        gen.name.clone(),
                                                    ),
                                                ));
                                            }
                                        }
                                        StreamScopeRef::Empty(_) => {}
                                    }
                                }
                            }
                        }
                    }
                });
            ui.add(
                Button::new(icons::FONT_SIZE).selected(!self.variable_name_filter_case_insensitive),
            )
            .on_hover_text("Case sensitive filter")
            .clicked()
            .then(|| {
                msgs.push(Message::SetVariableNameFilterCaseInsensitive(
                    !self.variable_name_filter_case_insensitive,
                ));
            });
            ui.menu_button(icons::FILTER_FILL, |ui| {
                variable_name_filter_type_menu(ui, msgs, &self.variable_name_filter_type);
            });
            ui.add_enabled(!filter.is_empty(), Button::new(icons::CLOSE_FILL))
                .on_hover_text("Clear filter")
                .clicked()
                .then(|| filter.clear());

            // Check if regex and if an incorrect regex, change background color
            if self.variable_name_filter_type == VariableNameFilterType::Regex
                && Regex::new(filter).is_err()
            {
                ui.style_mut().visuals.extreme_bg_color = self.config.theme.accent_error.background;
            }
            // Create text edit
            let response =
                ui.add(TextEdit::singleline(filter).hint_text("Filter (context menu for type)"));
            response.context_menu(|ui| {
                variable_name_filter_type_menu(ui, msgs, &self.variable_name_filter_type);
            });
            // Handle focus
            if response.gained_focus() {
                msgs.push(Message::SetFilterFocused(true));
            }
            if response.lost_focus() {
                msgs.push(Message::SetFilterFocused(false));
            }
            ui.spacing_mut().button_padding = default_padding;
        });
    }

    pub fn filtered_variables(&self, variables: &[VariableRef], filter: &str) -> Vec<VariableRef> {
        if filter.is_empty() {
            variables
                .iter()
                .sorted_by(|a, b| numeric_sort::cmp(&a.name, &b.name))
                .cloned()
                .collect_vec()
        } else {
            self.variable_name_filter_type
                .matching_variables(
                    variables,
                    filter,
                    self.variable_name_filter_case_insensitive,
                )
                .iter()
                .sorted_by(|a, b| numeric_sort::cmp(&a.name, &b.name))
                .cloned()
                .collect_vec()
        }
    }
}

pub fn variable_name_filter_type_menu(
    ui: &mut Ui,
    msgs: &mut Vec<Message>,
    variable_name_filter_type: &VariableNameFilterType,
) {
    for filter_type in enum_iterator::all::<VariableNameFilterType>() {
        ui.radio(
            *variable_name_filter_type == filter_type,
            filter_type.to_string(),
        )
        .clicked()
        .then(|| {
            ui.close_menu();
            msgs.push(Message::SetVariableNameFilterType(filter_type));
        });
    }
}
