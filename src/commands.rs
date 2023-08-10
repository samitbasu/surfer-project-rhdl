use std::fs;

use crate::{
    util::{alpha_idx_to_uint_idx, uint_idx_to_alpha_idx},
    Message, State,
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
                    v.inner.signal_from_signal_idx(s.0).name()
                )
            })
            .collect_vec(),
        None => vec![],
    };

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
        vec!["add_signal", "add_scope", "focus", "unfocus", "load_vcd"]
            .into_iter()
            .map(|s| s.into())
            .collect(),
        Box::new(move |query, _| match query {
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
            _ => None,
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
