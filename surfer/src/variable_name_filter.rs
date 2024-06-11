use eframe::egui::{Button, Layout, TextEdit, Ui};
use eframe::emath::{Align, Vec2};
use egui_remixicon::icons;
use enum_iterator::Sequence;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use itertools::Itertools;
use regex::{escape, Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::wave_container::ScopeRef;
use crate::{message::Message, wave_container::VariableRef, wave_data::WaveData, State};

#[derive(Debug, PartialEq, Serialize, Deserialize, Sequence)]
pub enum VariableNameFilterType {
    Fuzzy,
    Regex,
    Start,
    Contain,
}

impl std::fmt::Display for VariableNameFilterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VariableNameFilterType::Fuzzy => write!(f, "Fuzzy"),
            VariableNameFilterType::Regex => write!(f, "Regular expression"),
            VariableNameFilterType::Start => write!(f, "Variable starts with"),
            VariableNameFilterType::Contain => write!(f, "Variable contains"),
        }
    }
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
                            msgs.push(Message::AddVariables(self.filtered_variables(
                                waves,
                                filter,
                                active_scope,
                            )))
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
                ))
            });
            ui.menu_button(icons::FILTER_FILL, |ui| {
                variable_name_filter_type_menu(ui, msgs, &self.variable_name_filter_type)
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
            let response = ui
                .add(TextEdit::singleline(filter).hint_text("Filter (context menu for type)"))
                .context_menu(|ui| {
                    variable_name_filter_type_menu(ui, msgs, &self.variable_name_filter_type)
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

    pub fn filtered_variables(
        &self,
        waves: &WaveData,
        filter: &str,
        scope: &ScopeRef,
    ) -> Vec<VariableRef> {
        if filter.is_empty() {
            waves
                .inner
                .variables_in_scope(scope)
                .iter()
                .sorted_by(|a, b| numeric_sort::cmp(&a.name, &b.name))
                .cloned()
                .collect_vec()
        } else {
            self.variable_name_filter_type
                .matching_variables(
                    &waves.inner.variables_in_scope(scope),
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