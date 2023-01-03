use std::collections::BTreeMap;

use fastwave_backend::{Signal, SignalValue, SignalIdx};
use num::{BigInt, FromPrimitive};
use num::{BigRational, BigUint};

use crate::viewport::Viewport;
use crate::{Message, State};

impl State {
    fn draw(
        &self,
        _interaction: &Interaction,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(bounds.size());
        let background = Path::rectangle(Point::ORIGIN, frame.size());
        frame.fill(&background, Color::from_rgb8(0, 0, 0));

        frame.scale(1.);
        frame.translate(Vector::new(0., 0.));

        for x in 0..100 {
            frame.fill_rectangle(
                Point::new(x as f32 * 10., 0.),
                Size::new(1., frame.height()),
                Color::from_rgb8(10, 10, 10),
            );
        }

        let mut prev_values = BTreeMap::new();

        let cfg = DrawConfig {
            line_height: 16.,
            padding: 4.,
        };

        let max_time = BigRational::from_integer(self.num_timestamps.clone());

        if let Some(vcd) = &self.vcd {
            let frame_width = frame.width();
            'outer: for x in 0..frame_width as u32 {
                let time = self
                    .viewport_to_time(BigRational::from_float(x as f64).unwrap(), frame.width());
                if time < BigRational::from_float(0.).unwrap() {
                    continue;
                }
                let is_last_x = time > max_time;
                let time = time.to_integer().to_biguint().unwrap();

                for (y, (idx, sig)) in self
                    .signals
                    .iter()
                    .map(|s| (s, vcd.signal_from_signal_idx(*s)))
                    .enumerate()
                {
                    if let Ok(val) = sig.query_val_on_tmln(&time, &vcd) {
                        let prev = prev_values.get(idx);
                        if let Some((old_x, old_val)) = prev_values.get(idx) {
                            self.draw_signal(
                                &mut frame,
                                y as f32,
                                idx,
                                &sig,
                                (*old_x, old_val),
                                (x, &val),
                                &cfg,
                                // Force redraw on the last valid pixel to ensure
                                // that the signal gets drawn the whole way
                                x == (frame_width as u32 - 1) || is_last_x,
                            );
                        }

                        // Only store the last time if the value is actually changed
                        if prev.map(|(_, v)| v) != Some(&val) {
                            prev_values.insert(*idx, (x, val));
                        }

                        // If this was the last x in the vcd file, we are done
                        // drawing, so we can reak out of the outer loop
                    }
                }
                if is_last_x {
                    break 'outer;
                }
            }
        }

        vec![frame.into_geometry()]
    }
}

impl State {
    fn draw_signal(
        &self,
        frame: &mut Frame,
        y: f32,
        signal_idx: &SignalIdx,
        signal: &Signal,
        (old_x, old_val): (u32, &SignalValue),
        (x, val): (u32, &SignalValue),
        cfg: &DrawConfig,
        force_redraw: bool,
    ) {
        let y_start = y as f32 * (cfg.line_height + cfg.padding);
        let abs_point =
            |x: f32, rel_y: f32| Point::new(x as f32, y_start + (1. - rel_y) * cfg.line_height);

        if old_val != val || force_redraw {
            if signal.num_bits() == Some(1) {
                let (old_height, old_color) = old_val.bool_drawing_spec();
                let (new_height, _) = val.bool_drawing_spec();

                let mut path = PathBuilder::new();
                path.line_to(abs_point(old_x as f32, old_height));
                path.line_to(abs_point(x as f32, old_height));
                path.line_to(abs_point(x as f32, new_height));
                frame.stroke(
                    &path.build(),
                    Stroke::default().with_color(old_color).with_width(1.0),
                );

            } else {
                let mut border = PathBuilder::new();

                let transition_width = (x - old_x).min(6) as f32;
                border.line_to(abs_point(old_x as f32, 0.5));
                border.line_to(abs_point(old_x as f32 + transition_width / 2., 1.0));
                border.line_to(abs_point(x as f32 - transition_width/2., 1.0));
                border.line_to(abs_point(x as f32, 0.5));
                border.line_to(abs_point(x as f32 - transition_width/2., 0.0));
                border.line_to(abs_point(old_x as f32 + transition_width / 2., 0.0));
                border.line_to(abs_point(old_x as f32, 0.5));



                let stroke_color = match old_val.value_kind() {
                    ValueKind::HighImp => style::c_yellow(),
                    ValueKind::Undef => style::c_red(),
                    ValueKind::Normal => style::c_green(),
                };

                frame.stroke(
                    &border.build(),
                    Stroke::default().with_color(stroke_color).with_width(1.0),
                );

                let text_size = cfg.line_height - 4.;
                let char_width = text_size * 0.53;

                let text_area = (x - old_x) as f32 - transition_width;
                let num_chars = (text_area / char_width).floor();
                let fits_text =  num_chars >= 1.;

                if fits_text {
                    let translator_name = self.signal_format.get(&signal_idx)
                        .unwrap_or_else(|| &self.translators.default);
                    let translator = &self.translators.inner[translator_name];

                    // TODO: Graceful shutdown
                    let full_text = translator.translate(signal, old_val).unwrap().val;

                    let content = if full_text.len() > num_chars as usize {
                        full_text
                            .chars()
                            .take(num_chars as usize - 1)
                            .chain(['…'].into_iter())
                            .collect::<String>()
                    }
                    else {
                        full_text
                    };

                    let text = Text {
                        content,
                        position: abs_point(old_x as f32 + transition_width, 0.5),
                        color: Color::from_rgba(1., 1., 1., 1.),
                        size: text_size,
                        font: self.font,
                        vertical_alignment: iced::alignment::Vertical::Center,
                        .. Default::default()
                    };

                    frame.fill_text(text)
                }
            }
        }
    }

    fn viewport_to_time(&self, x: BigRational, view_width: f32) -> BigRational {
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self.viewport;

        let time_spacing = (right - left) / BigInt::from_u64(view_width as u64).unwrap();

        let time = left + time_spacing * x;
        time
    }

    pub fn handle_scroll(
        &self,
        cursor: Cursor,
        bounds: Rectangle,
        delta: ScrollDelta,
    ) -> Option<Message> {
        match delta {
            ScrollDelta::Lines { x: _, y } => {
                // Zoom or scroll
                if self.control_key {
                    if let Some(cursor_pos) = cursor.position_in(&bounds) {
                        let Viewport {
                            curr_left: left,
                            curr_right: right,
                            ..
                        } = &self.viewport;

                        let cursor_pos = self.viewport_to_time(
                            BigRational::from_float(cursor_pos.x).unwrap(),
                            bounds.width,
                        );

                        // - to get zoom in the natural direction
                        let scale = BigRational::from_float(1. - y / 10.).unwrap();

                        let target_left = (left - &cursor_pos) * &scale + &cursor_pos;
                        let target_right = (right - &cursor_pos) * &scale + &cursor_pos;

                        // TODO: Do not just round here, this will not work
                        // for small zoom levels
                        Some(Message::ChangeViewport(Viewport {
                            curr_left: target_left.clone().round(),
                            curr_right: target_right.clone().round(),
                        }))
                    } else {
                        None
                    }
                } else {
                    // Scroll 5% of the viewport per scroll event
                    let scroll_step = (&self.viewport.curr_right - &self.viewport.curr_left)
                        / BigInt::from_u32(20).unwrap();

                    let to_scroll = BigRational::from(scroll_step.clone())
                        * BigRational::from_float(y).unwrap();

                    let target_left = &self.viewport.curr_left + &to_scroll;
                    let target_right = &self.viewport.curr_right + &to_scroll;
                    Some(Message::ChangeViewport(Viewport {
                        curr_left: target_left.clone(),
                        curr_right: target_right.clone(),
                    }))
                }
            }
            ScrollDelta::Pixels { .. } => {
                // TODO
                println!("NOTE: Pixel scroll is unimplemented");
                None
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
    fn bool_drawing_spec(&self) -> (f32, Color);
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
    fn bool_drawing_spec(&self) -> (f32, Color) {
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
    use iced::Color;

    fn c_min() -> f32 {
        0.3
    }
    fn c_max() -> f32 {
        1.0
    }
    fn c_mid() -> f32 {
        0.5
    }

    pub fn c_green() -> Color {
        Color::from_rgb(c_min(), c_max(), c_min())
    }
    pub fn c_dark_green() -> Color {
        Color::from_rgb(c_min(), c_mid(), c_min())
    }

    pub fn c_red() -> Color {
        Color::from_rgb(c_max(), c_min(), c_min())
    }

    pub fn c_yellow() -> Color {
        Color::from_rgb(c_max(), c_max(), c_min())
    }
}
