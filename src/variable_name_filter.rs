use eframe::egui::{Layout, TextEdit, Ui};
use eframe::emath::Align;
use enum_iterator::Sequence;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use itertools::Itertools;
use regex::Regex;
use serde::{Deserialize, Serialize};

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
    pub fn is_match(&self, variable_name: &str, filter: &str) -> bool {
        match self {
            VariableNameFilterType::Fuzzy => {
                let matcher = SkimMatcherV2::default();
                matcher.fuzzy_match(variable_name, filter).is_some()
            }
            VariableNameFilterType::Contain => variable_name.contains(filter),
            VariableNameFilterType::Start => variable_name.starts_with(filter),
            VariableNameFilterType::Regex => {
                if let Ok(regex) = Regex::new(filter) {
                    regex.is_match(variable_name)
                } else {
                    false
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
            ui.button("➕")
                .on_hover_text("Add all variables")
                .clicked()
                .then(|| {
                    if let Some(waves) = self.waves.as_ref() {
                        // Iterate over the reversed list to get
                        // waves in the same order as the variable
                        // list
                        for var in
                            filtered_variables(waves, filter, &self.variable_name_filter_type)
                                .into_iter()
                                .rev()
                        {
                            msgs.push(Message::AddVariable(var))
                        }
                    }
                });
            ui.button("❌")
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
        });
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

pub fn filtered_variables(
    waves: &WaveData,
    filter: &str,
    variable_name_filter_type: &VariableNameFilterType,
) -> Vec<VariableRef> {
    if let Some(scope) = &waves.active_scope {
        let listed = waves
            .inner
            .variables_in_scope(scope)
            .iter()
            .filter(|var| variable_name_filter_type.is_match(&var.name, filter))
            .sorted_by(|a, b| human_sort::compare(&a.name, &b.name))
            .cloned()
            .collect_vec();

        listed
    } else {
        vec![]
    }
}
