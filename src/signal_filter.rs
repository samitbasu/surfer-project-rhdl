use eframe::{
    egui::{self, Ui},
    emath::Align,
};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use itertools::Itertools;
use regex::Regex;

use crate::{message::Message, wave_container::SignalRef, wave_data::WaveData, State};

#[derive(Debug, PartialEq)]
pub enum SignalFilterType {
    Fuzzy,
    Regex,
    Start,
    Contain,
}

impl std::fmt::Display for SignalFilterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalFilterType::Fuzzy => write!(f, "Fuzzy"),
            SignalFilterType::Regex => write!(f, "Regular expression"),
            SignalFilterType::Start => write!(f, "Signal starts with"),
            SignalFilterType::Contain => write!(f, "Signal contains"),
        }
    }
}

impl SignalFilterType {
    pub fn is_match(&self, signal_name: &str, filter: &str) -> bool {
        match self {
            SignalFilterType::Fuzzy => {
                let matcher = SkimMatcherV2::default();
                matcher.fuzzy_match(signal_name, filter).is_some()
            }
            SignalFilterType::Contain => signal_name.contains(filter),
            SignalFilterType::Start => signal_name.starts_with(filter),
            SignalFilterType::Regex => {
                if let Ok(regex) = Regex::new(filter) {
                    regex.is_match(signal_name)
                } else {
                    false
                }
            }
        }
    }
}

impl State {
    pub fn draw_signal_filter_edit(
        &self,
        ui: &mut egui::Ui,
        filter: &mut String,
        msgs: &mut Vec<Message>,
    ) {
        ui.with_layout(egui::Layout::right_to_left(Align::TOP), |ui| {
            ui.button("➕")
                .on_hover_text("Add all signals")
                .clicked()
                .then(|| {
                    if let Some(waves) = self.waves.as_ref() {
                        // Iterate over the reversed list to get
                        // waves in the same order as the signal
                        // list
                        for sig in filtered_signals(waves, filter, &self.signal_filter_type)
                            .into_iter()
                            .rev()
                        {
                            msgs.push(Message::AddSignal(sig))
                        }
                    }
                });
            ui.button("❌")
                .on_hover_text("Clear filter")
                .clicked()
                .then(|| filter.clear());

            // Check if regex and if an incorrect regex, change background color
            if self.signal_filter_type == SignalFilterType::Regex && Regex::new(filter).is_err() {
                ui.style_mut().visuals.extreme_bg_color = self.config.theme.accent_error.background;
            }
            // Create text edit
            let response = ui
                .add(egui::TextEdit::singleline(filter).hint_text("Filter (context menu for type)"))
                .context_menu(|ui| signal_filter_type_menu(ui, msgs, &self.signal_filter_type));
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

pub fn signal_filter_type_menu(
    ui: &mut Ui,
    msgs: &mut Vec<Message>,
    signal_filter_type: &SignalFilterType,
) {
    let filter_types = vec![
        SignalFilterType::Fuzzy,
        SignalFilterType::Regex,
        SignalFilterType::Start,
        SignalFilterType::Contain,
    ];
    for filter_type in filter_types {
        ui.radio(*signal_filter_type == filter_type, filter_type.to_string())
            .clicked()
            .then(|| {
                ui.close_menu();
                msgs.push(Message::SetSignalFilterType(filter_type));
            });
    }
}

pub fn filtered_signals(
    waves: &WaveData,
    filter: &str,
    signal_filter_type: &SignalFilterType,
) -> Vec<SignalRef> {
    if let Some(scope) = &waves.active_module {
        let listed = waves
            .inner
            .signals_in_module(scope)
            .iter()
            .filter(|sig| signal_filter_type.is_match(&sig.name, filter))
            .sorted_by(|a, b| human_sort::compare(&a.name, &b.name))
            .cloned()
            .collect_vec();

        listed
    } else {
        vec![]
    }
}
