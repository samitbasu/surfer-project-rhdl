use std::collections::BTreeMap;
use std::iter::zip;
use std::{fs, str::FromStr};

use eframe::emath::{Align, Align2, NumExt, Vec2};
use eframe::epaint::{FontFamily, FontId};
use egui::scroll_area::ScrollBarVisibility;
use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{Key, RichText, TextEdit};
use fzcmd::{expand_command, parse_command, Command, FuzzyOutput, ParamGreed, ParseError};
use itertools::Itertools;

use crate::config::{ArrowKeyBindings, HierarchyStyle};
use crate::displayed_item::DisplayedItemIndex;
use crate::wave_container::{ScopeRef, ScopeRefExt, VariableRef, VariableRefExt};
use crate::wave_data::ScopeType;
use crate::wave_source::LoadOptions;
use crate::{
    clock_highlighting::ClockHighlightType,
    displayed_item::DisplayedItem,
    message::Message,
    util::{alpha_idx_to_uint_idx, uint_idx_to_alpha_idx},
    variable_name_type::VariableNameType,
    State,
};

type RestCommand = Box<dyn Fn(&str) -> Option<Command<Message>>>;

pub fn get_parser(state: &State) -> Command<Message> {
    fn single_word(
        suggestions: Vec<String>,
        rest_command: RestCommand,
    ) -> Option<Command<Message>> {
        Some(Command::NonTerminal(
            ParamGreed::Rest,
            suggestions,
            Box::new(move |query, _| rest_command(query)),
        ))
    }

    fn optional_single_word(
        suggestions: Vec<String>,
        rest_command: RestCommand,
    ) -> Option<Command<Message>> {
        Some(Command::NonTerminal(
            ParamGreed::OptionalWord,
            suggestions,
            Box::new(move |query, _| rest_command(query)),
        ))
    }

    fn single_word_delayed_suggestions(
        suggestions: Box<dyn Fn() -> Vec<String>>,
        rest_command: RestCommand,
    ) -> Option<Command<Message>> {
        Some(Command::NonTerminal(
            ParamGreed::Rest,
            suggestions(),
            Box::new(move |query, _| rest_command(query)),
        ))
    }

    let scopes = match &state.waves {
        Some(v) => v.inner.scope_names(),
        None => vec![],
    };
    let variables = match &state.waves {
        Some(v) => v.inner.variable_names(),
        None => vec![],
    };
    let displayed_items = match &state.waves {
        Some(v) => v
            .displayed_items_order
            .iter()
            .enumerate()
            .map(|(idx, item_id)| {
                let idx = DisplayedItemIndex(idx);
                let item = &v.displayed_items[item_id];
                match item {
                    DisplayedItem::Variable(var) => format!(
                        "{}_{}",
                        uint_idx_to_alpha_idx(idx, v.displayed_items.len()),
                        var.variable_ref.full_path_string()
                    ),
                    _ => format!(
                        "{}_{}",
                        uint_idx_to_alpha_idx(idx, v.displayed_items.len()),
                        item.name()
                    ),
                }
            })
            .collect_vec(),
        None => vec![],
    };
    let variables_in_active_scope = state
        .waves
        .as_ref()
        .and_then(|waves| {
            waves
                .active_scope
                .as_ref()
                .map(|scope| waves.inner.variables_in_scope(scope))
        })
        .unwrap_or_default();

    let color_names = state.config.theme.colors.keys().cloned().collect_vec();

    let active_scope = state.waves.as_ref().and_then(|w| w.active_scope.clone());

    fn files_with_ext(matches: fn(&str) -> bool) -> Vec<String> {
        if let Ok(res) = fs::read_dir(".") {
            res.map(|res| res.map(|e| e.path()).unwrap_or_default())
                .filter(|file| {
                    file.extension().map_or(false, |extension| {
                        (matches)(extension.to_str().unwrap_or(""))
                    })
                })
                .map(|file| file.into_os_string().into_string().unwrap())
                .collect::<Vec<String>>()
        } else {
            vec![]
        }
    }

    fn all_wave_files() -> Vec<String> {
        files_with_ext(is_wave_file_extension)
    }

    let markers = if let Some(waves) = &state.waves {
        waves
            .displayed_items_order
            .iter()
            .map(|id| waves.displayed_items.get(id))
            .filter_map(|item| match item {
                Some(DisplayedItem::Marker(marker)) => Some((item.unwrap().name(), marker.idx)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>()
    } else {
        BTreeMap::new()
    };

    let keep_during_reload = state.config.behavior.keep_during_reload;
    let mut commands = if state.waves.is_some() {
        vec![
            "load_file",
            "switch_file",
            "variable_add",
            "item_focus",
            "item_set_color",
            "item_set_background_color",
            "item_unset_color",
            "item_unset_background_color",
            "item_unfocus",
            "item_rename",
            "zoom_fit",
            "scope_add",
            "scope_select",
            "divider_add",
            "config_reload",
            "theme_select",
            "reload",
            "remove_unavailable",
            "show_controls",
            "show_mouse_gestures",
            "show_quick_start",
            "show_logs",
            "load_url",
            "scroll_to_start",
            "scroll_to_end",
            "goto_start",
            "goto_end",
            "zoom_in",
            "zoom_out",
            "toggle_menu",
            "toggle_side_panel",
            "toggle_fullscreen",
            "toggle_tick_lines",
            "variable_add_from_scope",
            "variable_set_name_type",
            "variable_force_name_type",
            "preference_set_clock_highlight",
            "preference_set_hierarchy_style",
            "preference_set_arrow_key_bindings",
            "goto_marker",
            "save_state",
            "save_state_as",
            "timeline_add",
            "show_marker_window",
            "viewport_add",
            "viewport_remove",
            "transition_next",
            "transition_previous",
            "copy_value",
            "pause_simulation",
            "unpause_simulation",
            "undo",
            "redo",
            "exit",
        ]
    } else {
        vec![
            "load_file",
            "load_url",
            "config_reload",
            "theme_select",
            "toggle_menu",
            "toggle_side_panel",
            "toggle_fullscreen",
            "show_controls",
            "show_mouse_gestures",
            "show_quick_start",
            "show_logs",
            "exit",
        ]
    };
    #[cfg(feature = "performance_plot")]
    commands.push("show_performance");
    #[cfg(not(target_arch = "wasm32"))]
    commands.push("load_state");
    let mut theme_names = state.config.theme.theme_names.clone();
    let state_file = state.state_file.clone();
    theme_names.insert(0, "default".to_string());
    Command::NonTerminal(
        ParamGreed::Word,
        commands.into_iter().map(|s| s.into()).collect(),
        Box::new(move |query, _| {
            let variables_in_active_scope = variables_in_active_scope.clone();
            let markers = markers.clone();
            let scopes = scopes.clone();
            let active_scope = active_scope.clone();
            match query {
                "load_file" => single_word_delayed_suggestions(
                    Box::new(all_wave_files),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::LoadWaveformFile(
                            word.into(),
                            LoadOptions::clean(),
                        )))
                    }),
                ),
                "switch_file" => single_word_delayed_suggestions(
                    Box::new(all_wave_files),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::LoadWaveformFile(
                            word.into(),
                            LoadOptions {
                                keep_variables: true,
                                keep_unavailable: false,
                            },
                        )))
                    }),
                ),
                "load_url" => Some(Command::NonTerminal(
                    ParamGreed::Rest,
                    vec![],
                    Box::new(|query, _| {
                        Some(Command::Terminal(Message::LoadWaveformFileFromUrl(
                            query.to_string(),
                            LoadOptions::clean(), // load_url does not indicate any format restrictions
                        )))
                    }),
                )),
                "config_reload" => Some(Command::Terminal(Message::ReloadConfig)),
                "theme_select" => single_word(
                    theme_names.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::SelectTheme(Some(
                            word.to_owned(),
                        ))))
                    }),
                ),
                "scroll_to_start" | "goto_start" => {
                    Some(Command::Terminal(Message::GoToStart { viewport_idx: 0 }))
                }
                "scroll_to_end" | "goto_end" => {
                    Some(Command::Terminal(Message::GoToEnd { viewport_idx: 0 }))
                }
                "zoom_in" => Some(Command::Terminal(Message::CanvasZoom {
                    mouse_ptr: None,
                    delta: 0.5,
                    viewport_idx: 0,
                })),
                "zoom_out" => Some(Command::Terminal(Message::CanvasZoom {
                    mouse_ptr: None,
                    delta: 2.0,
                    viewport_idx: 0,
                })),
                "zoom_fit" => Some(Command::Terminal(Message::ZoomToFit { viewport_idx: 0 })),
                "toggle_menu" => Some(Command::Terminal(Message::ToggleMenu)),
                "toggle_side_panel" => Some(Command::Terminal(Message::ToggleSidePanel)),
                "toggle_fullscreen" => Some(Command::Terminal(Message::ToggleFullscreen)),
                "toggle_tick_lines" => Some(Command::Terminal(Message::ToggleTickLines)),
                // scope commands
                "scope_add" | "module_add" => single_word(
                    scopes,
                    Box::new(|word| {
                        Some(Command::Terminal(Message::AddScope(
                            ScopeRef::from_hierarchy_string(word),
                        )))
                    }),
                ),
                "scope_select" => single_word(
                    scopes.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::SetActiveScope(
                            ScopeType::WaveScope(ScopeRef::from_hierarchy_string(word)),
                        )))
                    }),
                ),
                "reload" => Some(Command::Terminal(Message::ReloadWaveform(
                    keep_during_reload,
                ))),
                "remove_unavailable" => Some(Command::Terminal(Message::RemovePlaceholders)),
                // Variable commands
                "variable_add" => single_word(
                    variables.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::AddVariables(vec![
                            VariableRef::from_hierarchy_string(word),
                        ])))
                    }),
                ),
                "variable_add_from_scope" => single_word(
                    variables_in_active_scope
                        .into_iter()
                        .map(|s| s.name())
                        .collect(),
                    Box::new(move |name| {
                        active_scope.as_ref().map(|scope| match scope {
                            ScopeType::WaveScope(w) => Command::Terminal(Message::AddVariables(
                                vec![VariableRef::new(w.clone(), name.to_string())],
                            )),
                            ScopeType::StreamScope(stream_scope) => {
                                Command::Terminal(Message::AddStreamOrGeneratorFromName(
                                    stream_scope.clone(),
                                    name.to_string(),
                                ))
                            }
                        })
                    }),
                ),
                "item_set_color" => single_word(
                    color_names.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::ItemColorChange(
                            None,
                            Some(word.to_string()),
                        )))
                    }),
                ),
                "item_set_background_color" => single_word(
                    color_names.clone(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::ItemBackgroundColorChange(
                            None,
                            Some(word.to_string()),
                        )))
                    }),
                ),
                "item_unset_color" => Some(Command::Terminal(Message::ItemColorChange(None, None))),
                "item_unset_background_color" => Some(Command::Terminal(
                    Message::ItemBackgroundColorChange(None, None),
                )),
                "item_rename" => Some(Command::Terminal(Message::RenameItem(None))),
                "variable_set_name_type" => single_word(
                    vec![
                        "Local".to_string(),
                        "Unique".to_string(),
                        "Global".to_string(),
                    ],
                    Box::new(|word| {
                        Some(Command::Terminal(Message::ChangeVariableNameType(
                            None,
                            VariableNameType::from_str(word).unwrap_or(VariableNameType::Local),
                        )))
                    }),
                ),
                "variable_force_name_type" => single_word(
                    vec![
                        "Local".to_string(),
                        "Unique".to_string(),
                        "Global".to_string(),
                    ],
                    Box::new(|word| {
                        Some(Command::Terminal(Message::ForceVariableNameTypes(
                            VariableNameType::from_str(word).unwrap_or(VariableNameType::Local),
                        )))
                    }),
                ),
                "item_focus" => single_word(
                    displayed_items.clone(),
                    Box::new(|word| {
                        // split off the idx which is always followed by an underscore
                        let alpha_idx: String = word.chars().take_while(|c| *c != '_').collect();
                        alpha_idx_to_uint_idx(alpha_idx)
                            .map(|idx| Command::Terminal(Message::FocusItem(idx)))
                    }),
                ),
                "transition_next" => single_word(
                    displayed_items.clone(),
                    Box::new(|word| {
                        // split off the idx which is always followed by an underscore
                        let alpha_idx: String = word.chars().take_while(|c| *c != '_').collect();
                        alpha_idx_to_uint_idx(alpha_idx).map(|idx| {
                            Command::Terminal(Message::MoveCursorToTransition {
                                next: true,
                                variable: Some(idx),
                                skip_zero: false,
                            })
                        })
                    }),
                ),
                "transition_previous" => single_word(
                    displayed_items.clone(),
                    Box::new(|word| {
                        // split off the idx which is always followed by an underscore
                        let alpha_idx: String = word.chars().take_while(|c| *c != '_').collect();
                        alpha_idx_to_uint_idx(alpha_idx).map(|idx| {
                            Command::Terminal(Message::MoveCursorToTransition {
                                next: false,
                                variable: Some(idx),
                                skip_zero: false,
                            })
                        })
                    }),
                ),
                "copy_value" => single_word(
                    displayed_items.clone(),
                    Box::new(|word| {
                        // split off the idx which is always followed by an underscore
                        let alpha_idx: String = word.chars().take_while(|c| *c != '_').collect();
                        alpha_idx_to_uint_idx(alpha_idx).map(|idx| {
                            Command::Terminal(Message::VariableValueToClipbord(Some(idx)))
                        })
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
                "preference_set_hierarchy_style" => single_word(
                    enum_iterator::all::<HierarchyStyle>()
                        .map(|o| o.to_string())
                        .collect_vec(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::SetHierarchyStyle(
                            HierarchyStyle::from(word.to_string()),
                        )))
                    }),
                ),
                "preference_set_arrow_key_bindings" => single_word(
                    enum_iterator::all::<ArrowKeyBindings>()
                        .map(|o| o.to_string())
                        .collect_vec(),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::SetArrowKeyBindings(
                            ArrowKeyBindings::from(word.to_string()),
                        )))
                    }),
                ),
                "item_unfocus" => Some(Command::Terminal(Message::UnfocusItem)),
                "divider_add" => optional_single_word(
                    vec![],
                    Box::new(|word| {
                        Some(Command::Terminal(Message::AddDivider(
                            Some(word.into()),
                            None,
                        )))
                    }),
                ),
                "timeline_add" => Some(Command::Terminal(Message::AddTimeLine(None))),
                "goto_marker" => single_word(
                    markers.keys().cloned().collect(),
                    Box::new(move |name| {
                        markers
                            .get(name)
                            .map(|idx| Command::Terminal(Message::GoToMarkerPosition(*idx, 0)))
                    }),
                ),
                "show_controls" => Some(Command::Terminal(Message::SetKeyHelpVisible(true))),
                "show_mouse_gestures" => {
                    Some(Command::Terminal(Message::SetGestureHelpVisible(true)))
                }
                "show_quick_start" => Some(Command::Terminal(Message::SetQuickStartVisible(true))),
                #[cfg(feature = "performance_plot")]
                "show_performance" => optional_single_word(
                    vec![],
                    Box::new(|word| {
                        if word == "redraw" {
                            Some(Command::Terminal(Message::Batch(vec![
                                Message::SetPerformanceVisible(true),
                                Message::SetContinuousRedraw(true),
                            ])))
                        } else {
                            Some(Command::Terminal(Message::SetPerformanceVisible(true)))
                        }
                    }),
                ),
                "show_marker_window" => {
                    Some(Command::Terminal(Message::SetCursorWindowVisible(true)))
                }
                "show_logs" => Some(Command::Terminal(Message::SetLogsVisible(true))),
                "save_state" => Some(Command::Terminal(Message::SaveStateFile(
                    state_file.clone(),
                ))),
                "save_state_as" => single_word(
                    vec![],
                    Box::new(|word| {
                        Some(Command::Terminal(Message::SaveStateFile(Some(
                            std::path::Path::new(word).into(),
                        ))))
                    }),
                ),
                "load_state" => single_word(
                    vec![],
                    Box::new(|word| {
                        Some(Command::Terminal(Message::LoadStateFile(Some(
                            std::path::Path::new(word).into(),
                        ))))
                    }),
                ),
                "viewport_add" => Some(Command::Terminal(Message::AddViewport)),
                "viewport_remove" => Some(Command::Terminal(Message::RemoveViewport)),
                "pause_simulation" => Some(Command::Terminal(Message::PauseSimulation)),
                "unpause_simulation" => Some(Command::Terminal(Message::UnpauseSimulation)),
                "undo" => Some(Command::Terminal(Message::Undo(1))),
                "redo" => Some(Command::Terminal(Message::Redo(1))),
                "exit" => Some(Command::Terminal(Message::Exit)),
                _ => None,
            }
        }),
    )
}

fn is_wave_file_extension(ext: &str) -> bool {
    ext == "vcd" || ext == "fst" || ext == "ghw"
}

pub fn run_fuzzy_parser(input: &str, state: &State, msgs: &mut Vec<Message>) {
    let FuzzyOutput {
        expanded: _,
        suggestions,
    } = expand_command(input, get_parser(state));

    msgs.push(Message::CommandPromptUpdate {
        suggestions: suggestions.unwrap_or_else(|_| vec![]),
    })
}

#[derive(Default)]
pub struct CommandPrompt {
    pub visible: bool,
    pub suggestions: Vec<(String, Vec<bool>)>,
    pub selected: usize,
    pub new_selection: Option<usize>,
    pub previous_commands: Vec<(String, Vec<bool>)>,
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
                let input = &mut *state.sys.command_prompt_text.borrow_mut();
                let response = ui.add(
                    TextEdit::singleline(input)
                        .desired_width(f32::INFINITY)
                        .lock_focus(true),
                );

                if response.changed() || state.sys.command_prompt.suggestions.is_empty() {
                    run_fuzzy_parser(input, state, msgs);
                }

                let set_cursor_to_pos = |pos, ui: &mut egui::Ui| {
                    if let Some(mut state) = TextEdit::load_state(ui.ctx(), response.id) {
                        let ccursor = CCursor::new(pos);
                        state
                            .cursor
                            .set_char_range(Some(CCursorRange::one(ccursor)));
                        state.store(ui.ctx(), response.id);
                        ui.ctx().memory_mut(|m| m.request_focus(response.id));
                    }
                };

                if response.ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
                    set_cursor_to_pos(input.chars().count(), ui);
                }

                let suggestions = state
                    .sys
                    .command_prompt
                    .previous_commands
                    .iter()
                    // take up to 3 previous commands
                    .take(if input.is_empty() { 3 } else { 0 })
                    // reverse them so that the most recent one is at the bottom
                    .rev()
                    .chain(state.sys.command_prompt.suggestions.iter())
                    .enumerate()
                    // allow scrolling down the suggestions
                    .collect_vec();

                // Expand the current input to full command and append the suggestion that is selected in the ui.
                let append_suggestion = |input: &String| -> String {
                    let new_input = if !state.sys.command_prompt.suggestions.is_empty() {
                        // if no suggestions exist we use the last argument in the input (e.g., for divider_add)
                        let default = input
                            .split_ascii_whitespace()
                            .last()
                            .unwrap_or("")
                            .to_string();

                        let selection = suggestions
                            .get(state.sys.command_prompt.selected)
                            .map(|s| &s.1 .0)
                            .unwrap_or(&default);

                        if input.chars().last().is_some_and(|c| c.is_whitespace()) {
                            // if no input exists for current argument just append
                            input.to_owned() + " " + selection
                        } else {
                            // if something was already typed for this argument removed then append
                            let parts = input.split_ascii_whitespace().collect_vec();
                            parts.iter().take(parts.len().saturating_sub(1)).join(" ")
                                + " "
                                + selection
                        }
                    } else {
                        input.to_string()
                    };
                    expand_command(&new_input, get_parser(state)).expanded
                };

                if response.ctx.input(|i| i.key_pressed(Key::Tab)) {
                    let mut new_input = append_suggestion(input);
                    let parsed = parse_command(&new_input, get_parser(state));
                    if let Err(ParseError::MissingParameters) = parsed {
                        new_input += " ";
                    }
                    *input = new_input;
                    set_cursor_to_pos(input.chars().count(), ui);
                    run_fuzzy_parser(input, state, msgs);
                }

                if response.lost_focus() && response.ctx.input(|i| i.key_pressed(Key::Enter)) {
                    let expanded = append_suggestion(input);
                    let parsed = (
                        expanded.clone(),
                        parse_command(&expanded, get_parser(state)),
                    );

                    if let Ok(cmd) = parsed.1 {
                        msgs.push(Message::ShowCommandPrompt(false));
                        msgs.push(Message::CommandPromptClear);
                        msgs.push(Message::CommandPromptPushPrevious(parsed.0));
                        msgs.push(cmd);
                        run_fuzzy_parser("", state, msgs);
                    } else {
                        *input = parsed.0 + " ";
                        // move cursor to end of input
                        set_cursor_to_pos(input.chars().count(), ui);
                        // run fuzzy parser since setting the cursor swallows the `changed` flag
                        run_fuzzy_parser(input, state, msgs);
                    }
                }

                response.request_focus();

                // draw current expansion of input and selected suggestions
                let expanded = expand_command(input, get_parser(state)).expanded;
                if !expanded.is_empty() {
                    ui.horizontal(|ui| {
                        let label = ui.label(
                            RichText::new("Expansion").color(
                                state
                                    .config
                                    .theme
                                    .primary_ui_color
                                    .foreground
                                    .gamma_multiply(0.5),
                            ),
                        );
                        ui.vertical(|ui| {
                            ui.add_space(label.rect.height() / 2.0);
                            ui.separator()
                        });
                    });

                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::top_down(Align::LEFT).with_cross_justify(true),
                        |ui| {
                            ui.add(SuggestionLabel::new(
                                RichText::new(expanded.clone())
                                    .size(14.0)
                                    .family(FontFamily::Monospace)
                                    .color(
                                        state
                                            .config
                                            .theme
                                            .accent_info
                                            .background
                                            .gamma_multiply(0.75),
                                    ),
                                false,
                            ))
                        },
                    );
                }

                egui::ScrollArea::vertical()
                    .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        for (idx, suggestion) in suggestions {
                            let mut job = LayoutJob::default();
                            let selected = state.sys.command_prompt.selected == idx;

                            let previous_cmds_len =
                                state.sys.command_prompt.previous_commands.len();
                            if idx == 0 && previous_cmds_len != 0 && input.is_empty() {
                                ui.horizontal(|ui| {
                                    let label = ui.label(
                                        RichText::new("Recently used").color(
                                            state
                                                .config
                                                .theme
                                                .primary_ui_color
                                                .foreground
                                                .gamma_multiply(0.5),
                                        ),
                                    );
                                    ui.vertical(|ui| {
                                        ui.add_space(label.rect.height() / 2.0);
                                        ui.separator()
                                    });
                                });
                            }

                            if (idx == previous_cmds_len.clamp(0, 3) && input.is_empty())
                                || (idx == 0 && !input.is_empty())
                            {
                                ui.horizontal(|ui| {
                                    let label = ui.label(
                                        RichText::new("Suggestions").color(
                                            state
                                                .config
                                                .theme
                                                .primary_ui_color
                                                .foreground
                                                .gamma_multiply(0.5),
                                        ),
                                    );
                                    ui.vertical(|ui| {
                                        ui.add_space(label.rect.height() / 2.0);
                                        ui.separator()
                                    });
                                });
                            }

                            for (c, highlight) in zip(suggestion.0.chars(), &suggestion.1) {
                                let mut tmp = [0u8; 4];
                                let sub_string = c.encode_utf8(&mut tmp);
                                job.append(
                                    sub_string,
                                    0.0,
                                    TextFormat {
                                        font_id: FontId::new(14.0, FontFamily::Monospace),
                                        color: if selected || *highlight {
                                            state.config.theme.accent_info.background
                                        } else {
                                            state.config.theme.primary_ui_color.foreground
                                        },
                                        ..Default::default()
                                    },
                                );
                            }

                            // make label full width of the palette
                            let resp = ui.allocate_ui_with_layout(
                                ui.available_size(),
                                egui::Layout::top_down(Align::LEFT).with_cross_justify(true),
                                |ui| ui.add(SuggestionLabel::new(job, selected)),
                            );

                            if state
                                .sys
                                .command_prompt
                                .new_selection
                                .is_some_and(|new_idx| idx == new_idx)
                            {
                                resp.response.scroll_to_me(Some(Align::Center));
                            }

                            if resp.inner.clicked() {
                                let new_input =
                                    if input.chars().last().is_some_and(|c| c.is_whitespace()) {
                                        // if no input exists for current argument just append
                                        input.to_owned() + " " + &suggestion.0
                                    } else {
                                        // if something was already typed for this argument removed then append
                                        let parts = input.split_ascii_whitespace().collect_vec();
                                        parts.iter().take(parts.len().saturating_sub(1)).join(" ")
                                            + " "
                                            + &suggestion.0
                                    };
                                let expanded =
                                    expand_command(&new_input, get_parser(state)).expanded;
                                let result = (
                                    expanded.clone(),
                                    parse_command(&expanded, get_parser(state)),
                                );

                                if let Ok(cmd) = result.1 {
                                    msgs.push(Message::ShowCommandPrompt(false));
                                    msgs.push(Message::CommandPromptClear);
                                    msgs.push(Message::CommandPromptPushPrevious(expanded));
                                    msgs.push(cmd);
                                    run_fuzzy_parser("", state, msgs);
                                } else {
                                    *input = result.0 + " ";
                                    set_cursor_to_pos(input.chars().count(), ui);
                                    // run fuzzy parser since setting the cursor swallows the `changed` flag
                                    run_fuzzy_parser(input, state, msgs);
                                }
                            }
                        }
                    });
            });
        });
}

// This SuggestionLabel is based on egui's SelectableLabel
#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct SuggestionLabel {
    text: egui::WidgetText,
    selected: bool,
}

impl SuggestionLabel {
    pub fn new(text: impl Into<egui::WidgetText>, selected: bool) -> Self {
        Self {
            text: text.into(),
            selected,
        }
    }
}

impl egui::Widget for SuggestionLabel {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let Self { text, selected: _ } = self;

        let button_padding = ui.spacing().button_padding;
        let total_extra = button_padding + button_padding;

        let wrap_width = ui.available_width() - total_extra.x;
        let text = text.into_galley(ui, None, wrap_width, egui::TextStyle::Button);

        let mut desired_size = total_extra + text.size();
        desired_size.y = desired_size.y.at_least(ui.spacing().interact_size.y);
        let (rect, response) = ui.allocate_at_least(desired_size, egui::Sense::click());

        if ui.is_rect_visible(response.rect) {
            let text_pos = ui
                .layout()
                .align_size_within_rect(text.size(), rect.shrink2(button_padding))
                .min;

            let visuals = ui.style().interact_selectable(&response, false);

            if response.hovered() || self.selected {
                let rect = rect.expand(visuals.expansion);

                ui.painter().rect(
                    rect,
                    visuals.rounding,
                    visuals.weak_bg_fill,
                    egui::Stroke::NONE,
                );
            }

            ui.painter().galley(text_pos, text, visuals.text_color());
        }

        response
    }
}
