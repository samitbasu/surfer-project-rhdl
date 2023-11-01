use eframe::egui::Ui;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use regex::Regex;

use crate::message::Message;

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
