use crate::{Message, State};

use fzcmd::{expand_command, Command, FuzzyOutput, ParamGreed};

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

    Command::NonTerminal(
        ParamGreed::Word,
        vec!["add_signal", "add_scope"]
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
