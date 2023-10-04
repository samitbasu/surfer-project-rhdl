use std::iter::zip;

use eframe::egui::{self};
use eframe::emath::Align2;
use eframe::epaint::Vec2;
use eframe::epaint::{FontFamily, FontId};
use egui::text::{LayoutJob, TextFormat};
use fzcmd::parse_command;

use crate::{
    commands::{get_parser, run_fuzzy_parser},
    Message, State,
};

pub struct CommandPrompt {
    pub visible: bool,
    pub input: String,
    pub expanded: String,
    pub suggestions: Vec<(String, Vec<bool>)>,
}

pub fn show_command_prompt(
    state: &mut State,
    ctx: &egui::Context,
    // Window size if known. If unknown defaults to a width of 200pts
    window_size: Option<Vec2>,
    msgs: &mut Vec<Message>,
) {
    egui::Window::new("Commands")
        .anchor(Align2::CENTER_TOP, Vec2::ZERO)
        .title_bar(false)
        .min_width({ window_size.map(|s| s.x * 0.3).unwrap_or(200.) })
        .resizable(true)
        .show(ctx, |ui| {
            egui::Frame::none().show(ui, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    ui.colored_label(state.config.theme.primary_ui_color.foreground, "🏄");

                    let response = ui.add(
                        egui::TextEdit::singleline(&mut state.command_prompt.input)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true),
                    );

                    if response.changed() {
                        run_fuzzy_parser(state);
                    }

                    if response.lost_focus()
                        && response.ctx.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        let command_parsed =
                            parse_command(&state.command_prompt.expanded, get_parser(state)).ok();

                        if command_parsed.is_some() {
                            msgs.push(Message::ShowCommandPrompt(false));
                            msgs.push(command_parsed.unwrap());
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
