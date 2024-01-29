use std::collections::BTreeMap;
use std::iter::zip;
use std::{fs, str::FromStr};

use eframe::egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use eframe::egui::{self, Align, Key, NumExt, RichText, TextEdit};
use eframe::emath::Align2;
use eframe::epaint::{FontFamily, FontId, Vec2};
use fzcmd::{expand_command, parse_command, Command, FuzzyOutput, ParamGreed};
use itertools::Itertools;

use crate::wave_source::{LoadOptions, WaveFormat};
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

    fn optional_single_word(
        suggestions: Vec<String>,
        rest_command: Box<dyn Fn(&str) -> Option<Command<Message>>>,
    ) -> Option<Command<Message>> {
        Some(Command::NonTerminal(
            ParamGreed::OptionalWord,
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
        Some(v) => v.inner.module_names(),
        None => vec![],
    };
    let signals = match &state.waves {
        Some(v) => v.inner.signal_names(),
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

    fn vcd_files() -> Vec<String> {
        files_with_ext(is_vcd_extension)
    }
    fn all_wave_files() -> Vec<String> {
        files_with_ext(is_wave_file_extension)
    }

    let cursors = if let Some(waves) = &state.waves {
        waves
            .displayed_items
            .iter()
            .filter_map(|item| match item {
                DisplayedItem::Cursor(cursor) => Some((item.name(), cursor.idx)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>()
    } else {
        BTreeMap::new()
    };

    let keep_during_reload = state.config.behavior.keep_during_reload;

    Command::NonTerminal(
        ParamGreed::Word,
        if state.waves.is_some() {
            vec![
                "load_vcd",
                "load_file",
                "signal_add",
                "signal_focus",
                "signal_set_color",
                "zoom_fit",
                "module_add",
                "module_select",
                "divider_add",
                "config_reload",
                "reload",
                "remove_unavailable",
                "show_controls",
                "show_mouse_gestures",
                "show_quick_start",
                "show_performance",
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
                "signal_add_from_module",
                "signal_set_name_type",
                "signal_force_name_type",
                "signal_unfocus",
                "signal_unset_color",
                "preference_set_clock_highlight",
                "goto_cursor",
                "save_state",
                "timeline_add",
                "show_cursor_window",
                "exit",
            ]
        } else {
            vec![
                "load_vcd",
                "load_file",
                "load_url",
                "config_reload",
                "toggle_menu",
                "toggle_side_panel",
                "toggle_fullscreen",
                "show_controls",
                "show_mouse_gestures",
                "show_quick_start",
                "show_performance",
                "exit",
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
                    Box::new(|word| {
                        Some(Command::Terminal(Message::LoadWaveformFile(
                            word.into(),
                            LoadOptions::clean_with_expected_format(WaveFormat::Vcd),
                        )))
                    }),
                ),
                "load_file" => single_word_delayed_suggestions(
                    Box::new(all_wave_files),
                    Box::new(|word| {
                        Some(Command::Terminal(Message::LoadWaveformFile(
                            word.into(),
                            LoadOptions::clean(),
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
                "toggle_side_panel" => Some(Command::Terminal(Message::ToggleSidePanel)),
                "toggle_fullscreen" => Some(Command::Terminal(Message::ToggleFullscreen)),
                "toggle_tick_lines" => Some(Command::Terminal(Message::ToggleTickLines)),
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
                "reload" => Some(Command::Terminal(Message::ReloadWaveform(
                    keep_during_reload,
                ))),
                "remove_unavailable" => Some(Command::Terminal(Message::RemovePlaceholders)),
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
                "goto_cursor" => single_word(
                    cursors.keys().cloned().collect(),
                    Box::new(move |name| {
                        cursors
                            .get(name)
                            .map(|idx| Command::Terminal(Message::GoToCursorPosition(*idx)))
                    }),
                ),
                "show_controls" => Some(Command::Terminal(Message::SetKeyHelpVisible(true))),
                "show_mouse_gestures" => {
                    Some(Command::Terminal(Message::SetGestureHelpVisible(true)))
                }
                "show_quick_start" => Some(Command::Terminal(Message::SetQuickStartVisible(true))),
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
                "show_cursor_window" => {
                    Some(Command::Terminal(Message::SetCursorWindowVisible(true)))
                }
                "save_state" => single_word(
                    vec![],
                    Box::new(|word| Some(Command::Terminal(Message::SaveState(word.into())))),
                ),
                "exit" => Some(Command::Terminal(Message::Exit)),
                _ => None,
            }
        }),
    )
}

fn is_vcd_extension(ext: &str) -> bool {
    ext == "vcd"
}
fn is_wave_file_extension(ext: &str) -> bool {
    ext == "vcd" || ext == "fst"
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
                        state.set_ccursor_range(Some(CCursorRange::one(ccursor)));
                        state.store(ui.ctx(), response.id);
                        ui.ctx().memory_mut(|m| m.request_focus(response.id));
                    }
                };

                if response.ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
                    set_cursor_to_pos(input.chars().count(), ui);
                }

                let skip_suggestions = state.sys.command_prompt.selected.saturating_sub(14);
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
                    .skip(skip_suggestions)
                    .take(15)
                    .collect_vec();

                let expanded = expand_command(input, get_parser(state)).expanded;
                if response.lost_focus() && response.ctx.input(|i| i.key_pressed(Key::Enter)) {
                    let new_input = if !state.sys.command_prompt.suggestions.is_empty() {
                        // if no suggestions exist we use the last argument in the input (e.g., for divider_add)
                        let default = (
                            0,
                            &(
                                input
                                    .split_ascii_whitespace()
                                    .last()
                                    .unwrap_or("")
                                    .to_string(),
                                vec![false; input.len()],
                            ),
                        );

                        let selection = suggestions
                            .get(state.sys.command_prompt.selected - skip_suggestions)
                            .unwrap_or(&default);

                        if input.chars().last().is_some_and(|c| c.is_whitespace()) {
                            // if no input exists for current argument just append
                            input.to_owned() + " " + &selection.1 .0
                        } else {
                            // if something was already typed for this argument removed then append
                            let parts = input.split_ascii_whitespace().collect_vec();
                            parts.iter().take(parts.len().saturating_sub(1)).join(" ")
                                + " "
                                + &selection.1 .0
                        }
                    } else {
                        input.to_string()
                    };

                    let expanded = expand_command(&new_input, get_parser(state)).expanded;
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

                for (idx, suggestion) in suggestions {
                    let mut job = LayoutJob::default();
                    let selected = state.sys.command_prompt.selected == idx;

                    let previous_cmds_len = state.sys.command_prompt.previous_commands.len();
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

                    if resp.inner.clicked() {
                        let new_input = if input.chars().last().is_some_and(|c| c.is_whitespace()) {
                            // if no input exists for current argument just append
                            input.to_owned() + " " + &suggestion.0
                        } else {
                            // if something was already typed for this argument removed then append
                            let parts = input.split_ascii_whitespace().collect_vec();
                            parts.iter().take(parts.len().saturating_sub(1)).join(" ")
                                + " "
                                + &suggestion.0
                        };
                        let expanded = expand_command(&new_input, get_parser(state)).expanded;
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
