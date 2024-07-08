use std::str::FromStr;

use eframe::emath::{Pos2, Rect};
use eframe::epaint::Stroke;
use egui::Ui;
use enum_iterator::Sequence;
use serde::Deserialize;

use crate::{config::SurferConfig, message::Message, view::DrawingContext};

#[derive(PartialEq, Copy, Clone, Debug, Deserialize, Sequence)]
pub enum ClockHighlightType {
    Line,  // Draw a line at every posedge of the clokcs
    Cycle, // Highlight every other cycle
    None,  // No highlighting
}

impl FromStr for ClockHighlightType {
    type Err = String;

    fn from_str(input: &str) -> Result<ClockHighlightType, Self::Err> {
        match input {
            "Line" => Ok(ClockHighlightType::Line),
            "Cycle" => Ok(ClockHighlightType::Cycle),
            "None" => Ok(ClockHighlightType::None),
            _ => Err(format!(
                "'{}' is not a valid ClockHighlightType (Valid options: Line|Cycle|None)",
                input
            )),
        }
    }
}

impl std::fmt::Display for ClockHighlightType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClockHighlightType::Line => write!(f, "Line"),
            ClockHighlightType::Cycle => write!(f, "Cycle"),
            ClockHighlightType::None => write!(f, "None"),
        }
    }
}

pub fn draw_clock_edge(
    x_start: f32,
    x_end: f32,
    cycle: bool,
    ctx: &mut DrawingContext,
    config: &SurferConfig,
) {
    match config.default_clock_highlight_type {
        ClockHighlightType::Line => {
            let Pos2 {
                x: x_pos,
                y: y_start,
            } = (ctx.to_screen)(x_start, 0.);
            ctx.painter.vline(
                x_pos,
                (y_start)..=(y_start + ctx.cfg.canvas_height),
                Stroke {
                    color: config.theme.clock_highlight_line.color,
                    width: config.theme.clock_highlight_line.width,
                },
            );
        }
        ClockHighlightType::Cycle => {
            if cycle {
                let Pos2 {
                    x: x_end,
                    y: y_start,
                } = (ctx.to_screen)(x_end, 0.);
                ctx.painter.rect_filled(
                    Rect {
                        min: (ctx.to_screen)(x_start, 0.),
                        max: Pos2 {
                            x: x_end,
                            y: ctx.cfg.canvas_height + y_start,
                        },
                    },
                    0.0,
                    config.theme.clock_highlight_cycle,
                );
            }
        }
        ClockHighlightType::None => (),
    }
}

pub fn clock_highlight_type_menu(
    ui: &mut Ui,
    msgs: &mut Vec<Message>,
    clock_highlight_type: ClockHighlightType,
) {
    for highlight_type in enum_iterator::all::<ClockHighlightType>() {
        ui.radio(
            highlight_type == clock_highlight_type,
            highlight_type.to_string(),
        )
        .clicked()
        .then(|| {
            ui.close_menu();
            msgs.push(Message::SetClockHighlightType(highlight_type));
        });
    }
}
