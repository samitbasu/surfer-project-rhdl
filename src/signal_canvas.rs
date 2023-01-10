use std::collections::{BTreeMap, HashMap};

use eframe::egui::{self, Painter, Sense};
use eframe::emath::{self, Align2, RectTransform};
use eframe::epaint::{Color32, FontId, PathShape, Pos2, Rect, Rounding, Stroke, Vec2};
use fastwave_backend::{Signal, SignalIdx, SignalValue};
use num::FromPrimitive;
use num::{BigRational, BigUint};

use crate::translation::TranslatorList;
use crate::view::TraceIdx;
use crate::{Message, State, VcdData};

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

        let mut prev_values = BTreeMap::new();

        let cfg = DrawConfig {
            line_height: 16.,
            padding: 4.,
        };

        let max_time = BigRational::from_integer(vcd.num_timestamps.clone());

        vcd.draw_cursor(&mut painter, response.rect.size(), to_screen);

        'outer: for x in 0..frame_width as u32 {
            let time = vcd.viewport.to_time(x as f64, frame_width);
            if time < BigRational::from_float(0.).unwrap() {
                continue;
            }
            let is_last_x = time > max_time;
            let time = time.to_integer().to_biguint().unwrap();

            for (idx, sig) in vcd
                .signals
                .iter()
                .map(|s| (s, vcd.inner.signal_from_signal_idx(s.0)))
            {
                if let Ok(val) = sig.query_val_on_tmln(&time, &vcd.inner) {
                    let y = signal_offsets
                        .get(&(idx.0, vec![]))
                        .expect(&format!("Found no y offset for signal {}", sig.name()));

                    let prev = prev_values.get(&idx.0);
                    if let Some((old_x, old_val)) = prev_values.get(&idx.0) {
                        vcd.draw_signal(
                            &mut painter,
                            to_screen
                                .inverse()
                                .transform_pos(Pos2::new(0., *y as f32))
                                .y,
                            to_screen,
                            &idx.0,
                            &sig,
                            (*old_x, old_val),
                            (x, &val),
                            &cfg,
                            // Force redraw on the last valid pixel to ensure
                            // that the signal gets drawn the whole way
                            x == (frame_width as u32 - 1) || is_last_x,
                            &self.translators,
                        );
                    }

                    // Only store the last time if the value is actually changed
                    if prev.map(|(_, v)| v) != Some(&val) {
                        prev_values.insert(idx.0, (x, val));
                    }
                }
            }

            // If this was the last x in the vcd file, we are done
            // drawing, so we can reak out of the outer loop
            if is_last_x {
                break 'outer;
            }
        }
    }
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
        painter: &mut Painter,
        y: f32,
        to_screen: RectTransform,
        signal_idx: &SignalIdx,
        signal: &Signal,
        (old_x, old_val): (u32, &SignalValue),
        (x, val): (u32, &SignalValue),
        cfg: &DrawConfig,
        force_redraw: bool,
        translators: &TranslatorList,
    ) {
        let abs_point = |x: f32, rel_y: f32| {
            to_screen.transform_pos(Pos2::new(x as f32, y + (1. - rel_y) * cfg.line_height))
        };

        if old_val != val || force_redraw {
            if signal.num_bits() == Some(1) {
                let (old_height, old_color) = old_val.bool_drawing_spec();
                let (new_height, _) = val.bool_drawing_spec();

                let stroke = Stroke {
                    color: old_color,
                    width: 1.,
                    ..Default::default()
                };

                painter.add(PathShape::line(
                    vec![
                        abs_point(old_x as f32, old_height),
                        abs_point(x as f32, old_height),
                        abs_point(x as f32, new_height),
                    ],
                    stroke,
                ));
            } else {
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

                let transition_width = (x - old_x).min(6) as f32;

                painter.add(PathShape::line(
                    vec![
                        abs_point(old_x as f32, 0.5),
                        abs_point(old_x as f32 + transition_width / 2., 1.0),
                        abs_point(x as f32 - transition_width / 2., 1.0),
                        abs_point(x as f32, 0.5),
                        abs_point(x as f32 - transition_width / 2., 0.0),
                        abs_point(old_x as f32 + transition_width / 2., 0.0),
                        abs_point(old_x as f32, 0.5),
                    ],
                    stroke,
                ));

                let text_size = cfg.line_height - 5.;
                let char_width = text_size * (18. / 31.);

                let text_area = (x - old_x) as f32 - transition_width;
                let num_chars = (text_area / char_width).floor();
                let fits_text = num_chars >= 1.;

                if fits_text {
                    let translator = self.signal_translator(*signal_idx, translators);

                    // TODO: Graceful shutdown
                    let full_text = translator.translate(signal, old_val).unwrap().val;

                    let content = if full_text.len() > num_chars as usize {
                        full_text
                            .chars()
                            .take(num_chars as usize - 1)
                            .chain(['…'].into_iter())
                            .collect::<String>()
                    } else {
                        full_text
                    };

                    painter.text(
                        abs_point(old_x as f32 + transition_width, 0.5),
                        Align2::LEFT_CENTER,
                        content,
                        FontId::monospace(text_size),
                        Color32::from_rgb(255, 255, 255),
                    );
                }
            }
        }
    }
}

struct DrawConfig {
    line_height: f32,
    padding: f32,
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
