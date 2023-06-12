use std::collections::HashMap;

use eframe::egui::{self, Painter, Sense};
use eframe::emath::{self, Align2, RectTransform};
use eframe::epaint::{Color32, FontId, PathShape, Pos2, Rect, Rounding, Stroke, Vec2};
use log::error;
use num::BigRational;

use crate::benchmark::{TimedRegion, TranslationTimings};
use crate::translation::SignalInfo;
use crate::view::TraceIdx;
use crate::{Message, State, VcdData};

struct DrawnRegion {
    color: Color32,
    value: String,
}

/// List of values to draw for a signal. It is an ordered list of values that should
/// be drawn at the *start time* until the *start time* of the next value
struct DrawingCommands {
    is_bool: bool,
    values: Vec<(f32, DrawnRegion)>,
}

impl DrawingCommands {
    pub fn new_bool() -> Self {
        Self {
            values: vec![],
            is_bool: true,
        }
    }

    pub fn new_wide() -> Self {
        Self {
            values: vec![],
            is_bool: false,
        }
    }

    pub fn push(&mut self, val: (f32, DrawnRegion)) {
        self.values.push(val)
    }
}

impl State {
    pub fn draw_signals(
        &self,
        msgs: &mut Vec<Message>,
        signal_offsets: &HashMap<TraceIdx, f32>,
        vcd: &VcdData,
        ui: &mut egui::Ui,
    ) {
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::hover());

        let container_rect = Rect::from_min_size(Pos2::ZERO, response.rect.size());
        let to_screen = emath::RectTransform::from_to(container_rect, response.rect);
        let frame_width = response.rect.width();

        // TODO: Move event handling into its own function
        // TODO: Consider using events instead of querying like this
        let pointer_pos_global = ui.input().pointer.interact_pos();
        let pointer_pos_canvas = pointer_pos_global.map(|p| to_screen.inverse().transform_pos(p));

        let pointer_in_canvas = pointer_pos_global
            .map(|p| to_screen.transform_rect(container_rect).contains(p))
            .unwrap_or(false);

        if pointer_in_canvas {
            let pointer_pos = pointer_pos_global.unwrap();
            let scroll_delta = ui.input().scroll_delta;
            let mouse_ptr_pos = to_screen.inverse().transform_pos(pointer_pos);
            if scroll_delta != Vec2::ZERO {
                msgs.push(Message::CanvasScroll {
                    delta: ui.input().scroll_delta,
                })
            }

            if ui.input().zoom_delta() != 1. {
                let mouse_ptr_timestamp = vcd.viewport.to_time(mouse_ptr_pos.x as f64, frame_width);

                msgs.push(Message::CanvasZoom {
                    mouse_ptr_timestamp,
                    delta: ui.input().zoom_delta(),
                })
            }

            ui.input().pointer.primary_down().then(|| {
                let x = pointer_pos_canvas.unwrap().x;
                let timestamp = vcd.viewport.to_time(x as f64, frame_width);
                msgs.push(Message::CursorSet(timestamp.round().to_integer()));
            });
        }

        painter.rect_filled(response.rect, Rounding::none(), Color32::from_rgb(0, 0, 0));

        let cfg = DrawConfig {
            line_height: 16.,
            max_transition_width: 6,
        };

        let max_time = BigRational::from_integer(vcd.num_timestamps.clone());

        vcd.draw_cursor(&mut painter, response.rect.size(), to_screen);

        // Compute which timestamp to draw in each pixel. We'll draw from -transition_width to
        // width + transition_width in order to draw initial transitions outside the screen
        let timestamps = (-cfg.max_transition_width
            ..(frame_width as i32 + cfg.max_transition_width))
            .filter_map(|x| {
                let time = vcd.viewport.to_time(x as f64, frame_width);
                if time < BigRational::from_float(0.).unwrap() {
                    None
                } else if time > max_time {
                    None
                } else {
                    Some((x as f32, time.to_integer().to_biguint().unwrap()))
                }
            })
            .collect::<Vec<_>>();

        let mut timings = TranslationTimings::new();

        let draw_commands = vcd
            .signals
            .iter()
            .map(|s| (s, vcd.inner.signal_from_signal_idx(s.0)))
            // Iterate over the signals, generating draw commands for all the
            // subfields
            .map(|((idx, info), sig)| {
                let translator = vcd.signal_translator((*idx, vec![]), &self.translators);

                let mut local_commands: HashMap<Vec<_>, _> = HashMap::new();

                let mut prev_values = HashMap::new();

                // In order to insert a final draw command at the end of a trace,
                // we need to know if this is the last timestamp to draw
                let end_pixel = timestamps.iter().last().map(|t| t.0).unwrap_or_default();
                // The first pixel we actually draw is the second pixel in the
                // list, since we skip one pixel to have a previous value
                let start_pixel = timestamps
                    .iter()
                    .skip(1)
                    .next()
                    .map(|t| t.0)
                    .unwrap_or_default();

                // Iterate over all the time stamps to draw on
                for ((_, prev_time), (pixel, time)) in
                    timestamps.iter().zip(timestamps.iter().skip(1))
                {
                    let (change_time, val) = if let Ok(v) = sig.query_val_on_tmln(&time, &vcd.inner)
                    {
                        v
                    } else {
                        // If there is no value here, skip this iteration
                        continue;
                    };

                    let is_last_timestep = pixel == &end_pixel;
                    let is_first_timestep = pixel == &start_pixel;

                    // Check if the value remains unchanged between this pixel
                    // and the last
                    if &change_time < prev_time && !is_first_timestep && !is_last_timestep {
                        continue;
                    }

                    // Perform the translation
                    let mut duration = TimedRegion::started();

                    let translation_result = match translator.translate(&sig, &val) {
                        Ok(result) => result,
                        Err(e) => {
                            error!(
                                "{translator_name} for {sig_name} failed. Disabling:",
                                translator_name = translator.name(),
                                sig_name = sig.name()
                            );
                            error!("{e:#}");
                            msgs.push(Message::ResetSignalFormat((*idx, vec![])));
                            return vec![];
                        }
                    };

                    duration.stop();
                    timings.push_timing(&translator.name(), None, duration.secs());

                    let fields = translation_result
                        .flatten((*idx, vec![]), &vcd.signal_format, &self.translators)
                        .as_fields();

                    for (path, value) in fields {
                        let prev = prev_values.get(&path);

                        // This is not the value we drew last time
                        if prev != Some(&value) || is_last_timestep {
                            *prev_values.entry(path.clone()).or_insert(value.clone()) =
                                value.clone();

                            // TODO: Use new_bool for bools
                            local_commands
                                .entry(path)
                                .or_insert_with(|| {
                                    if let SignalInfo::Bool = info {
                                        DrawingCommands::new_bool()
                                    } else {
                                        DrawingCommands::new_wide()
                                    }
                                })
                                .push((
                                    *pixel,
                                    DrawnRegion {
                                        value,
                                        color: Color32::GREEN,
                                    },
                                ))
                        }
                    }
                }

                // Append the signal index to the fields
                local_commands
                    .into_iter()
                    .map(|(path, val)| ((idx.clone(), path), val))
                    .collect::<Vec<_>>()
            })
            .flatten()
            .collect::<Vec<_>>();

        let mut ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y)),
        };

        for (trace, commands) in &draw_commands {
            let offset = signal_offsets.get(trace);
            if let Some(offset) = offset {
                for (old, new) in commands.values.iter().zip(commands.values.iter().skip(1)) {
                    if commands.is_bool {
                        self.draw_bool_transition((old, new), *offset, &mut ctx)
                    } else {
                        self.draw_region((old, new), *offset, &mut ctx)
                    }
                }
            }
        }

        egui::Window::new("Translation timings")
            .anchor(Align2::RIGHT_BOTTOM, Vec2::ZERO)
            .show(ui.ctx(), |ui| ui.label(timings.format()));
    }

    fn draw_region(
        &self,
        ((old_x, prev_region), (new_x, _)): (&(f32, DrawnRegion), &(f32, DrawnRegion)),
        offset: f32,
        ctx: &mut DrawingContext,
    ) {
        let stroke = Stroke {
            color: prev_region.color,
            width: 1.,
            ..Default::default()
        };

        let transition_width = (new_x - old_x).min(6.) as f32;

        let trace_coords = |x, y| (ctx.to_screen)(x, y * ctx.cfg.line_height + offset);

        ctx.painter.add(PathShape::line(
            vec![
                trace_coords(*old_x, 0.5),
                trace_coords(old_x + transition_width / 2., 1.0),
                trace_coords(new_x - transition_width / 2., 1.0),
                trace_coords(*new_x, 0.5),
                trace_coords(new_x - transition_width / 2., 0.0),
                trace_coords(old_x + transition_width / 2., 0.0),
                trace_coords(*old_x, 0.5),
            ],
            stroke,
        ));

        let text_size = ctx.cfg.line_height - 5.;
        let char_width = text_size * (18. / 31.);

        let text_area = (new_x - old_x) as f32 - transition_width;
        let num_chars = (text_area / char_width).floor();
        let fits_text = num_chars >= 1.;

        let full_text = &prev_region.value;
        if fits_text {
            let content = if full_text.len() > num_chars as usize {
                full_text
                    .chars()
                    .take(num_chars as usize - 1)
                    .chain(['…'].into_iter())
                    .collect::<String>()
            } else {
                full_text.to_string()
            };

            ctx.painter.text(
                trace_coords(*old_x + transition_width, 0.5),
                Align2::LEFT_CENTER,
                content,
                FontId::monospace(text_size),
                Color32::from_rgb(255, 255, 255),
            );
        }
    }

    fn draw_bool_transition(
        &self,
        ((old_x, prev_region), (new_x, new_region)): (&(f32, DrawnRegion), &(f32, DrawnRegion)),
        offset: f32,
        ctx: &mut DrawingContext,
    ) {
        let trace_coords = |x, y| (ctx.to_screen)(x, y * ctx.cfg.line_height + offset);

        let (old_height, old_color) = prev_region.value.bool_drawing_spec();
        let (new_height, _) = new_region.value.bool_drawing_spec();

        let stroke = Stroke {
            color: old_color,
            width: 1.,
            ..Default::default()
        };

        ctx.painter.add(PathShape::line(
            vec![
                trace_coords(*old_x, old_height),
                trace_coords(*new_x, old_height),
                trace_coords(*new_x, new_height),
            ],
            stroke,
        ));
    }
}

struct DrawingContext<'a> {
    painter: &'a mut Painter,
    cfg: &'a DrawConfig,
    to_screen: &'a dyn Fn(f32, f32) -> Pos2,
}

impl VcdData {
    fn draw_cursor(&self, painter: &mut Painter, size: Vec2, to_screen: RectTransform) {
        if let Some(cursor) = &self.cursor {
            let x = self.viewport.from_time(&cursor, size.x as f64);

            let stroke = Stroke {
                color: Color32::from_rgb(255, 128, 128),
                width: 2.,
                ..Default::default()
            };
            painter.line_segment(
                [
                    to_screen.transform_pos(Pos2::new(x as f32, 0.)),
                    to_screen.transform_pos(Pos2::new(x as f32, size.y)),
                ],
                stroke,
            )
        }
    }
}

struct DrawConfig {
    line_height: f32,
    max_transition_width: i32,
}

enum ValueKind {
    HighImp,
    Undef,
    Normal,
}

trait SignalExt {
    fn value_kind(&self) -> ValueKind;
    fn bool_drawing_spec(&self) -> (f32, Color32);
}

impl SignalExt for String {
    fn value_kind(&self) -> ValueKind {
        if self.to_lowercase().contains("x") {
            ValueKind::Undef
        } else if self.to_lowercase().contains("z") {
            ValueKind::HighImp
        } else {
            ValueKind::Normal
        }
    }

    /// Return the height and color with which to draw this value if it is a boolean
    fn bool_drawing_spec(&self) -> (f32, Color32) {
        match (self.value_kind(), self) {
            (ValueKind::HighImp, _) => (0.5, style::c_yellow()),
            (ValueKind::Undef, _) => (0.5, style::c_red()),
            (ValueKind::Normal, other) => {
                if other == "0" {
                    (0., style::c_dark_green())
                } else {
                    (1., style::c_green())
                }
            }
        }
    }
}

mod style {
    use eframe::epaint::Color32;

    fn c_min() -> u8 {
        64
    }
    fn c_max() -> u8 {
        255
    }
    fn c_mid() -> u8 {
        128
    }

    pub fn c_green() -> Color32 {
        Color32::from_rgb(c_min(), c_max(), c_min())
    }
    pub fn c_dark_green() -> Color32 {
        Color32::from_rgb(c_min(), c_mid(), c_min())
    }

    pub fn c_red() -> Color32 {
        Color32::from_rgb(c_max(), c_min(), c_min())
    }

    pub fn c_yellow() -> Color32 {
        Color32::from_rgb(c_max(), c_max(), c_min())
    }
}
