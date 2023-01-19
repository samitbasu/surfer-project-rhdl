use std::collections::{BTreeMap, HashMap};

use eframe::egui::{self, Painter, Sense};
use eframe::emath::{self, Align2, RectTransform};
use eframe::epaint::{Color32, FontId, PathShape, Pos2, Rect, Rounding, Stroke, Vec2};
use fastwave_backend::{SignalValue, SignalIdx, Signal};
use itertools::Itertools;
use log::error;
use num::FromPrimitive;
use num::{BigRational, BigUint};

use crate::benchmark::TimedRegion;
use crate::translation::{SignalInfo, TranslatorList};
use crate::view::TraceIdx;
use crate::{Message, State, VcdData};

struct TranslationTimings {
    timings: BTreeMap<String, (Vec<f64>, BTreeMap<String, Vec<f64>>)>,
}

impl TranslationTimings {
    fn new() -> Self {
        Self {
            timings: BTreeMap::new(),
        }
    }

    pub fn push_timing(&mut self, name: &str, subname: Option<&str>, timing: f64) {
        let target = self.timings.entry(name.to_string()).or_default();

        if let Some(subname) = subname {
            target
                .1
                .entry(subname.to_string())
                .or_default()
                .push(timing)
        }
        if subname.is_none() {
            target.0.push(timing)
        }
    }

    pub fn format(&self) -> String {
        self.timings
            .iter()
            .sorted_by_key(|(name, _)| name.as_str())
            .map(|(name, (counts, sub))| {
                let total: f64 = counts.iter().sum();
                let average = total / counts.len() as f64;

                let substr = sub
                    .iter()
                    .sorted_by_key(|(name, _)| name.as_str())
                    .map(|(name, counts)| {
                        let subtotal: f64 = counts.iter().sum();
                        let subaverage = total / counts.len() as f64;

                        let pct = (subtotal / total) * 100.;
                        format!("\t{name}: {subtotal:.05} {subaverage:.05} {pct:.05}%")
                    })
                    .join("\n");

                format!("{name}: {total:.05} ({average:.05})\n{substr}")
            })
            .join("\n")
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
        };

        let max_time = BigRational::from_integer(vcd.num_timestamps.clone());

        vcd.draw_cursor(&mut painter, response.rect.size(), to_screen);

        // Compute which timestamp to draw in each pixel
        let timestamps = (0..frame_width as u32).filter_map(|x| {
            let time = vcd.viewport.to_time(x as f64, frame_width);
            if time < BigRational::from_float(0.).unwrap() {
                None
            }
            else if time > max_time {
                None
            }
            else {
                Some((x as f32, time.to_integer().to_biguint().unwrap()))
            }
        }).collect::<Vec<_>>();



        let mut timings = TranslationTimings::new();

        for ((idx, info), sig) in vcd
            .signals
            .iter()
            .map(|s| (s, vcd.inner.signal_from_signal_idx(s.0)))
        {
            vcd.draw_signal(
                &timestamps,
                idx,
                info,
                &sig,
                &mut painter,
                to_screen,
                &cfg,
                &mut timings,
                signal_offsets,
                &self.translators,
                msgs
            );

        }

        egui::Window::new("Translation timings")
            .anchor(Align2::RIGHT_BOTTOM, Vec2::ZERO)
            .show(ui.ctx(), |ui| ui.label(timings.format()));
    }
}

struct DrawingContext<'a> {
    painter: &'a mut Painter,
    cfg: &'a DrawConfig,
    abs_point: &'a dyn Fn(f32, f32) -> Pos2,
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

    fn draw_signal(
        &self,
        timestamps: &[(f32, BigUint)],
        idx: &SignalIdx,
        info: &SignalInfo,
        signal: &Signal,
        painter: &mut Painter,
        to_screen: RectTransform,
        cfg: &DrawConfig,
        timings: &mut TranslationTimings,
        signal_offsets: &HashMap<TraceIdx, f32>,
        translators: &TranslatorList,
        msgs: &mut Vec<Message>,
    ) {
        let mut prev_value: Option<(f32, SignalValue)> = None;

        for (i, (x, time)) in timestamps.iter().enumerate() {
            if let Ok(val) = signal.query_val_on_tmln(&time, &self.inner) {
                let y = signal_offsets
                    .get(&(*idx, vec![]))
                    .expect(&format!("Found no y offset for signal {}", signal.name()));

                let abs_point = |x: f32, rel_y: f32| {
                    to_screen
                        .transform_pos(Pos2::new(x as f32, y + (1. - rel_y) * cfg.line_height))
                };

                let ctx = DrawingContext {
                    painter,
                    cfg: &cfg,
                    abs_point: &abs_point,
                };

                if let Some((old_x, old_val)) = &prev_value {
                    // Force redraw on the last valid pixel to ensure
                    // that the signal gets drawn the whole way
                    let force_redraw = i == timestamps.len()-1;
                    let draw = force_redraw || old_val != &val;

                    if draw {
                        let mut duration = TimedRegion::started();
                        let translator = self.signal_translator(*idx, &translators);

                        let translation_result = match translator.translate(&signal, &old_val) {
                            Ok(result) => result,
                            Err(e) => {
                                error!(
                                    "{translator_name} for {sig_name} failed. Disabling:",
                                    translator_name = translator.name(),
                                    sig_name = signal.name()
                                );
                                error!("{e:#}");
                                msgs.push(Message::ResetSignalFormat(*idx));
                                return;
                            }
                        };

                        duration.stop();
                        timings.push_timing(&translator.name(), None, duration.secs());
                        for (subname, time) in &translation_result.durations {
                            timings.push_timing(
                                &translator.name(),
                                Some(subname.as_str()),
                                *time,
                            )
                        }

                        let is_bool = signal.num_bits().unwrap_or(0) == 1;

                        if is_bool {
                            self.draw_bool_transition((*old_x, &old_val), (*x, &val), &ctx);
                        } else {
                            self.draw_transition(
                                (*old_x, &old_val),
                                *x,
                                &ctx,
                                &translation_result.to_string(),
                            )
                        }
                    }
                }

                // Only store the last time if the value is actually changed
                if prev_value.as_ref().map(|(_, v)| v != &val).unwrap_or(true) {
                    prev_value = Some((*x, val));
                }
            }
        }
    }

    fn draw_bool_transition(
        &self,
        (old_x, old_val): (f32, &SignalValue),
        (new_x, new_val): (f32, &SignalValue),
        ctx: &DrawingContext,
    ) {
        let abs_point = &ctx.abs_point;
        let (old_height, old_color) = old_val.bool_drawing_spec();
        let (new_height, _) = new_val.bool_drawing_spec();

        let stroke = Stroke {
            color: old_color,
            width: 1.,
            ..Default::default()
        };

        ctx.painter.add(PathShape::line(
            vec![
                abs_point(old_x as f32, old_height),
                abs_point(new_x as f32, old_height),
                abs_point(new_x as f32, new_height),
            ],
            stroke,
        ));
    }

    fn draw_transition(
        &self,
        (old_x, old_val): (f32, &SignalValue),
        new_x: f32,
        ctx: &DrawingContext,
        full_text: &str,
    ) {
        let abs_point = ctx.abs_point;

        let stroke_color = match old_val.value_kind() {
            ValueKind::HighImp => style::c_yellow(),
            ValueKind::Undef => style::c_red(),
            ValueKind::Normal => style::c_green(),
        };

        let stroke = Stroke {
            color: stroke_color,
            width: 1.,
            ..Default::default()
        };

        let transition_width = (new_x - old_x).min(6.) as f32;

        ctx.painter.add(PathShape::line(
            vec![
                abs_point(old_x, 0.5),
                abs_point(old_x + transition_width / 2., 1.0),
                abs_point(new_x - transition_width / 2., 1.0),
                abs_point(new_x, 0.5),
                abs_point(new_x - transition_width / 2., 0.0),
                abs_point(old_x + transition_width / 2., 0.0),
                abs_point(old_x, 0.5),
            ],
            stroke,
        ));

        let text_size = ctx.cfg.line_height - 5.;
        let char_width = text_size * (18. / 31.);

        let text_area = (new_x - old_x) as f32 - transition_width;
        let num_chars = (text_area / char_width).floor();
        let fits_text = num_chars >= 1.;

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
                abs_point(old_x as f32 + transition_width, 0.5),
                Align2::LEFT_CENTER,
                content,
                FontId::monospace(text_size),
                Color32::from_rgb(255, 255, 255),
            );
        }
    }
}

struct DrawConfig {
    line_height: f32,
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

impl SignalExt for SignalValue {
    fn value_kind(&self) -> ValueKind {
        match self {
            SignalValue::BigUint(_) => ValueKind::Normal,
            SignalValue::String(s) => {
                let s_lower = s.to_lowercase();
                if s_lower.contains("z") {
                    ValueKind::HighImp
                } else if s_lower.contains("x") {
                    ValueKind::Undef
                } else {
                    ValueKind::Normal
                }
            }
        }
    }

    /// Return the height and color with which to draw this value if it is a boolean
    fn bool_drawing_spec(&self) -> (f32, Color32) {
        match (self.value_kind(), self) {
            (ValueKind::HighImp, _) => (0.5, style::c_yellow()),
            (ValueKind::Undef, _) => (0.5, style::c_red()),
            (ValueKind::Normal, SignalValue::BigUint(num)) => {
                if num == &BigUint::from_u32(0).unwrap() {
                    (0., style::c_dark_green())
                } else {
                    (1., style::c_green())
                }
            }
            (ValueKind::Normal, SignalValue::String(_)) => {
                unreachable!()
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
