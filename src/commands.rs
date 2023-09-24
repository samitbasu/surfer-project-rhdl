use std::{collections::BTreeMap, fs, str::FromStr};

use crate::{
    util::{alpha_idx_to_uint_idx, uint_idx_to_alpha_idx},
    Message, SignalNameType, State,
};

use fzcmd::{expand_command, Command, FuzzyOutput, ParamGreed};
use itertools::Itertools;

pub fn get_parser(state: &State) -> Command<Message> {
    fn single_word(
        suggestions: Vec<String>,
        rest_command: Box<dyn Fn(&str) -> Option<Command<Message>>>,
    ) -> Option<Command<Message>> {
        Some(Command::NonTerminal(
            ParamGreed::Rest,
            suggestions,
            Box::new(move |query, _| rest_command(query)),
        ))
    }

    fn single_word_delayed_suggestions(
        suggestions: Box<dyn Fn() -> Vec<String>>,
        rest_command: Box<dyn Fn(&str) -> Option<Command<Message>>>,
    ) -> Option<Command<Message>> {
        Some(Command::NonTerminal(
            ParamGreed::Rest,
            suggestions(),
            Box::new(move |query, _| rest_command(query)),
        ))
    }

    let scopes = match &state.vcd {
        Some(v) => v
            .scopes_to_ids
            .keys()
            .map(|s| s.clone())
            .collect::<Vec<_>>(),
        None => vec![],
    };
    let signals = match &state.vcd {
        Some(v) => v
            .signals_to_ids
            .keys()
            .map(|s| s.clone())
            .collect::<Vec<_>>(),
        None => vec![],
    };
    let displayed_signals = match &state.vcd {
        Some(v) => v
            .signals
            .iter()
            .enumerate()
            .map(|(idx, s)| {
                format!(
                    "{}_{}",
                    uint_idx_to_alpha_idx(idx, v.signals.len()),
                    v.inner.signal_from_signal_idx(s.idx).name()
                )
            })
            .collect_vec(),
        None => vec![],
    };
    let signals_in_active_scope = state
        .vcd
        .as_ref()
        .and_then(|vcd| {
            vcd.active_scope.map(|scope| {
                vcd.inner
                    .get_children_signal_idxs(scope)
                    .into_iter()
                    .map(|signal_idx| {
                        (
                            vcd.inner.signal_from_signal_idx(signal_idx).name(),
                            signal_idx,
                        )
                    })
                    .collect::<BTreeMap<_, _>>()
            })
        })
        .unwrap_or_default();

    let color_names = state
        .config
        .theme
        .colors
        .keys()
        .map(|k| k.clone())
        .collect_vec();

    fn vcd_files() -> Vec<String> {
        if let Ok(res) = fs::read_dir(".") {
            res.map(|res| res.map(|e| e.path()).unwrap_or_default())
                .filter(|file| {
                    file.extension()
                        .map_or(false, |extension| extension.to_str().unwrap_or("") == "vcd")
                })
                .map(|file| file.into_os_string().into_string().unwrap())
                .collect::<Vec<String>>()
        } else {
            vec![]
        }
    }

    Command::NonTerminal(
        ParamGreed::Word,
        vec![
            "add_signal",
            "add_signal_from_scope",
            "add_scope",
            "select_scope",
            "focus",
            "unfocus",
            "load_vcd",
            "load_url",
            "set_signal_color",
            "reload_config",
            "scroll_to_start",
            "scroll_to_end",
            "zoom_in",
            "zoom_out",
            "zoom_to_fit",
            "toggle_menu",
            "signal_name_type_set",
        ]
        .into_iter()
        .map(|s| s.into())
        .collect(),
        Box::new(move |query, _| {
            let signals_in_active_scope = signals_in_active_scope.clone();
            match query {
                "add_signal" => single_word(
                    signals.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::AddSignal(
                            crate::SignalDescriptor::Name(word.into()),
                        )))
                    }),
                ),
                "add_scope" => single_word(
                    scopes.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::AddScope(
                            crate::ScopeDescriptor::Name(word.into()),
                        )))
                    }),
                ),
                "add_signal_from_scope" => single_word(
                    signals_in_active_scope.keys().cloned().collect(),
                    Box::new(move |name| {
                        signals_in_active_scope
                            .get(name)
                            .map(|idx| Command::Terminal(Message::AddSignal((*idx).into())))
                    }),
                ),
                "select_scope" => single_word(
                    scopes.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::SetActiveScope(
                            crate::ScopeDescriptor::Name(word.into()),
                        )))
                    }),
                ),
                "focus" => single_word(
                    displayed_signals.clone(),
                    Box::new(|word| {
                        // split off the idx which is always followed by an underscore
                        let alpha_idx: String = word.chars().take_while(|c| *c != '_').collect();
                        alpha_idx_to_uint_idx(alpha_idx)
                            .map(|idx| Command::Terminal(Message::FocusSignal(idx)))
                    }),
                ),
                "unfocus" => Some(Command::Terminal(Message::UnfocusSignal)),
                "load_vcd" => single_word_delayed_suggestions(
                    Box::new(vcd_files),
                    Box::new(|word| Some(Command::Terminal(Message::LoadVcd(word.into())))),
                ),
                "load_url" => Some(Command::NonTerminal(
                    ParamGreed::Rest,
                    vec![],
                    Box::new(|query, _| {
                        Some(Command::Terminal(Message::LoadVcdFromUrl(
                            query.to_string(),
                        )))
                    }),
                )),
                "set_signal_color" => single_word(
                    color_names.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::SignalColorChange(
                            None,
                            word.to_string(),
                        )))
                    }),
                ),
                "reload_config" => Some(Command::Terminal(Message::ReloadConfig)),
                "zoom_to_fit" => Some(Command::Terminal(Message::ZoomToFit)),
                "scroll_to_start" => Some(Command::Terminal(Message::ScrollToStart)),
                "scroll_to_end" => Some(Command::Terminal(Message::ScrollToEnd)),
                "zoom_in" => Some(Command::Terminal(Message::CanvasZoom {
                    mouse_ptr_timestamp: None,
                    delta: 0.5,
                })),
                "zoom_out" => Some(Command::Terminal(Message::CanvasZoom {
                    mouse_ptr_timestamp: None,
                    delta: 2.0,
                })),
                "toggle_menu" => Some(Command::Terminal(Message::ToggleMenu)),
                "signal_name_type_set" => single_word(
                    vec![
                        "Local".to_string(),
                        "Unique".to_string(),
                        "Global".to_string(),
                    ],
                    Box::new(|word| {
                        Some(Command::Terminal(Message::ChangeSignalNameType(
                            SignalNameType::from_str(word).unwrap_or(SignalNameType::Local),
                        )))
                    }),
                ),
                _ => None,
            }
        }),
    )
}

pub fn run_fuzzy_parser(state: &mut State) {
    let FuzzyOutput {
        expanded,
        suggestions,
    } = expand_command(&state.command_prompt.input, get_parser(state));

    state.command_prompt.expanded = expanded;
    state.command_prompt.suggestions = suggestions.unwrap_or(vec![]);
}
