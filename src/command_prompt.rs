use std::collections::BTreeMap;
use std::iter::zip;
use std::{fs, str::FromStr};

use eframe::egui::{self};
use eframe::emath::Align2;
use eframe::epaint::Vec2;
use eframe::epaint::{FontFamily, FontId};
use egui::text::{LayoutJob, TextFormat};
use fzcmd::{expand_command, parse_command, Command, FuzzyOutput, ParamGreed};
use itertools::Itertools;

use crate::{
    clock_highlighting::ClockHighlightType,
    displayed_item::DisplayedItem,
    message::Message,
    signal_name_type::SignalNameType,
    util::{alpha_idx_to_uint_idx, uint_idx_to_alpha_idx},
    wave_container::{ModuleRef, SignalRef},
    State,
};

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

    let modules = match &state.waves {
        Some(v) => v
            .inner
            .modules()
            .iter()
            .map(|module| format!("{module}"))
            .collect(),
        None => vec![],
    };
    let signals = match &state.waves {
        Some(v) => v
            .inner
            .signals()
            .iter()
            .map(|s| s.full_path_string())
            .collect(),
        None => vec![],
    };
    let displayed_signals = match &state.waves {
        Some(v) => v
            .displayed_items
            .iter()
            .filter_map(|item| match item {
                DisplayedItem::Signal(idx) => Some(idx),
                _ => None,
            })
            .enumerate()
            .map(|(idx, s)| {
                format!(
                    "{}_{}",
                    uint_idx_to_alpha_idx(idx, v.displayed_items.len()),
                    s.signal_ref.full_path_string()
                )
            })
            .collect_vec(),
        None => vec![],
    };
    let signals_in_active_scope = state
        .waves
        .as_ref()
        .and_then(|waves| {
            waves
                .active_module
                .as_ref()
                .map(|scope| waves.inner.signals_in_module(scope))
        })
        .unwrap_or_default();

    let color_names = state.config.theme.colors.keys().cloned().collect_vec();

    let active_module = state.waves.as_ref().and_then(|w| w.active_module.clone());

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

    let cursors = if let Some(waves) = &state.waves {
        waves
            .displayed_items
            .iter()
            .filter_map(|item| match item {
                DisplayedItem::Cursor(tmp_cursor) => Some(tmp_cursor),
                _ => None,
            })
            .map(|cursor| (cursor.name.clone(), cursor.idx))
            .collect::<BTreeMap<_, _>>()
    } else {
        BTreeMap::new()
    };

    Command::NonTerminal(
        ParamGreed::Word,
        if state.waves.is_some() {
            vec![
                "load_vcd",
                "signal_add",
                "signal_focus",
                "signal_set_color",
                "zoom_fit",
                "module_add",
                "module_select",
                "divider_add",
                "config_reload",
                "reload",
                "load_url",
                "scroll_to_start",
                "scroll_to_end",
                "goto_start",
                "goto_end",
                "zoom_in",
                "zoom_out",
                "toggle_menu",
                "toggle_fullscreen",
                "signal_add_from_module",
                "signal_set_name_type",
                "signal_force_name_type",
                "signal_unfocus",
                "signal_unset_color",
                "preference_set_clock_highlight",
                "goto_cursor",
            ]
        } else {
            vec![
                "load_vcd",
                "load_url",
                "config_reload",
                "toggle_menu",
                "toggle_fullscreen",
            ]
        }
        .into_iter()
        .map(|s| s.into())
        .collect(),
        Box::new(move |query, _| {
            let signals_in_active_scope = signals_in_active_scope.clone();
            let cursors = cursors.clone();
            let modules = modules.clone();
            let active_module = active_module.clone();
            match query {
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
                "config_reload" => Some(Command::Terminal(Message::ReloadConfig)),
                "scroll_to_start" | "goto_start" => Some(Command::Terminal(Message::GoToStart)),
                "scroll_to_end" | "goto_end" => Some(Command::Terminal(Message::GoToEnd)),
                "zoom_in" => Some(Command::Terminal(Message::CanvasZoom {
                    mouse_ptr_timestamp: None,
                    delta: 0.5,
                })),
                "zoom_out" => Some(Command::Terminal(Message::CanvasZoom {
                    mouse_ptr_timestamp: None,
                    delta: 2.0,
                })),
                "zoom_fit" => Some(Command::Terminal(Message::ZoomToFit)),
                "toggle_menu" => Some(Command::Terminal(Message::ToggleMenu)),
                "toggle_fullscreen" => Some(Command::Terminal(Message::ToggleFullscreen)),
                // Module commands
                "module_add" => single_word(
                    modules,
                    Box::new(|word| {
                        Some(Command::Terminal(Message::AddModule(
                            ModuleRef::from_hierarchy_string(word),
                        )))
                    }),
                ),
                "module_select" => single_word(
                    modules.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::SetActiveScope(
                            ModuleRef::from_hierarchy_string(word),
                        )))
                    }),
                ),
                "reload" => Some(Command::Terminal(Message::ReloadWaveform)),
                // Signal commands
                "signal_add" => single_word(
                    signals.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::AddSignal(
                            SignalRef::from_hierarchy_string(word),
                        )))
                    }),
                ),
                "signal_add_from_module" => single_word(
                    signals_in_active_scope
                        .into_iter()
                        .map(|s| s.name)
                        .collect(),
                    Box::new(move |name| {
                        active_module.as_ref().map(|module| {
                            Command::Terminal(Message::AddSignal(SignalRef::new(
                                module.clone(),
                                name.to_string(),
                            )))
                        })
                    }),
                ),
                "signal_set_color" => single_word(
                    color_names.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::ItemColorChange(
                            None,
                            Some(word.to_string()),
                        )))
                    }),
                ),
                "signal_unset_color" => {
                    Some(Command::Terminal(Message::ItemColorChange(None, None)))
                }
                "signal_set_name_type" => single_word(
                    vec![
                        "Local".to_string(),
                        "Unique".to_string(),
                        "Global".to_string(),
                    ],
                    Box::new(|word| {
                        Some(Command::Terminal(Message::ChangeSignalNameType(
                            None,
                            SignalNameType::from_str(word).unwrap_or(SignalNameType::Local),
                        )))
                    }),
                ),
                "signal_force_name_type" => single_word(
                    vec![
                        "Local".to_string(),
                        "Unique".to_string(),
                        "Global".to_string(),
                    ],
                    Box::new(|word| {
                        Some(Command::Terminal(Message::ForceSignalNameTypes(
                            SignalNameType::from_str(word).unwrap_or(SignalNameType::Local),
                        )))
                    }),
                ),
                "signal_focus" => single_word(
                    displayed_signals.clone(),
                    Box::new(|word| {
                        // split off the idx which is always followed by an underscore
                        let alpha_idx: String = word.chars().take_while(|c| *c != '_').collect();
                        alpha_idx_to_uint_idx(alpha_idx)
                            .map(|idx| Command::Terminal(Message::FocusItem(idx)))
                    }),
                ),
                "preference_set_clock_highlight" => single_word(
                    ["Line", "Cycle", "None"]
                        .iter()
                        .map(|o| o.to_string())
                        .collect_vec(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::SetClockHighlightType(
                            ClockHighlightType::from_str(word).unwrap_or(ClockHighlightType::Line),
                        )))
                    }),
                ),
                "signal_unfocus" => Some(Command::Terminal(Message::UnfocusItem)),
                "divider_add" => single_word(
                    vec![],
                    Box::new(|word| Some(Command::Terminal(Message::AddDivider(word.into())))),
                ),
                "goto_cursor" => single_word(
                    cursors.keys().cloned().collect(),
                    Box::new(move |name| {
                        cursors
                            .get(name)
                            .map(|idx| Command::Terminal(Message::GoToCursorPosition(*idx)))
                    }),
                ),
                _ => None,
            }
        }),
    )
}

pub fn run_fuzzy_parser(input: &str, state: &State, msgs: &mut Vec<Message>) {
    let FuzzyOutput {
        expanded,
        suggestions,
    } = expand_command(input, get_parser(state));

    msgs.push(Message::CommandPromptUpdate {
        expanded,
        suggestions: suggestions.unwrap_or(vec![]),
    })
}

pub struct CommandPrompt {
    pub visible: bool,
    pub expanded: String,
    pub suggestions: Vec<(String, Vec<bool>)>,
}

pub fn show_command_prompt(
    state: &State,
    ctx: &egui::Context,
    // Window size if known. If unknown defaults to a width of 200pts
    window_size: Option<Vec2>,
    msgs: &mut Vec<Message>,
) {
    egui::Window::new("Commands")
        .anchor(Align2::CENTER_TOP, Vec2::ZERO)
        .title_bar(false)
        .min_width(window_size.map(|s| s.x * 0.3).unwrap_or(200.))
        .resizable(true)
        .show(ctx, |ui| {
            egui::Frame::none().show(ui, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    ui.colored_label(state.config.theme.primary_ui_color.foreground, "🏄");

                    let input = &mut *state.command_prompt_text.borrow_mut();
                    let response = ui.add(
                        egui::TextEdit::singleline(input)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true),
                    );

                    if response.changed() || input.is_empty() {
                        run_fuzzy_parser(input, state, msgs);
                    }

                    if response.lost_focus()
                        && response.ctx.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        let command_parsed =
                            parse_command(&state.command_prompt.expanded, get_parser(state)).ok();

                        if let Some(command_parsed) = command_parsed {
                            msgs.push(Message::ShowCommandPrompt(false));
                            msgs.push(Message::CommandPromptClear);
                            msgs.push(command_parsed);
                        }
                    }

                    response.request_focus();
                });
            });

            ui.separator();

            // show expanded command below textedit
            if !state.command_prompt.expanded.is_empty() {
                let mut job = LayoutJob::default();
                // // indicate that the first row is selected
                job.append(
                    "↦ ",
                    0.0,
                    TextFormat {
                        font_id: FontId::new(14.0, FontFamily::Monospace),
                        color: state.config.theme.accent_info.background,
                        ..Default::default()
                    },
                );
                job.append(
                    &state.command_prompt.expanded,
                    0.0,
                    TextFormat {
                        font_id: FontId::new(14.0, FontFamily::Monospace),
                        color: state.config.theme.accent_info.background,
                        ..Default::default()
                    },
                );
                ui.label(job);
            }

            // only show the top 15 suggestions
            for suggestion in state.command_prompt.suggestions.iter().take(15) {
                let mut job = LayoutJob::default();
                job.append(
                    "  ",
                    0.0,
                    TextFormat {
                        font_id: FontId::new(14.0, FontFamily::Monospace),
                        color: state.config.theme.primary_ui_color.foreground,
                        ..Default::default()
                    },
                );

                for (c, highlight) in zip(suggestion.0.chars(), &suggestion.1) {
                    let mut tmp = [0u8; 4];
                    let sub_string = c.encode_utf8(&mut tmp);
                    job.append(
                        sub_string,
                        0.0,
                        TextFormat {
                            font_id: FontId::new(14.0, FontFamily::Monospace),
                            color: if *highlight {
                                state.config.theme.accent_info.background
                            } else {
                                state.config.theme.primary_ui_color.foreground
                            },
                            ..Default::default()
                        },
                    );
                }

                ui.label(job);
            }
        });
}
