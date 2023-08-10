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

    Command::NonTerminal(
        ParamGreed::Word,
        vec!["add_signal", "add_scope", "focus", "unfocus"]
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
