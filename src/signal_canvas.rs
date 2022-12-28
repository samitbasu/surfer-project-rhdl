use std::collections::BTreeMap;

use fastwave_backend::{Signal, SignalValue};
use iced::mouse::{Interaction, ScrollDelta};
use iced::widget::canvas::event::{self, Event};
use iced::widget::canvas::{self, Frame, Stroke};
use iced::widget::canvas::{Cursor, Geometry, Path};
use iced::{mouse, Color, Point, Rectangle, Size, Theme, Vector};
use num::{BigInt, FromPrimitive};
use num::{BigRational, BigUint};

use crate::viewport::Viewport;
use crate::{Message, State};

impl<'a> canvas::Program<Message> for State {
    type State = Interaction;

    fn update(
        &self,
        _interaction: &mut Interaction,
        event: Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> (event::Status, Option<Message>) {
        match event {
            Event::Mouse(m) => match m {
                mouse::Event::WheelScrolled { delta } => {
                    if cursor.is_over(&bounds) {
                        let msg = self.handle_scroll(cursor, bounds, delta);
                        (event::Status::Captured, msg)
                    } else {
                        (event::Status::Captured, None)
                    }
                }
                _ => (event::Status::Ignored, None),
            },
            Event::Touch(_) | Event::Keyboard(_) => (event::Status::Ignored, None),
        }
    }

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

        if let Some(vcd) = &self.vcd {
            for x in 0..frame.width() as u32 {
                let time = self
                    .viewport_to_time(BigRational::from_float(x as f64).unwrap(), frame.width());
                if time < BigRational::from_float(0.).unwrap() {
                    continue;
                }
                let time = time.to_integer().to_biguint().unwrap();

                let line_height = 20.;
                let padding = 4.;

                for (y, (idx, sig)) in self
                    .signals
                    .iter()
                    .map(|s| (s, vcd.signal_from_signal_idx(*s)))
                    .enumerate()
                {
                    if let Ok(val) = sig.query_val_on_tmln(&time, &vcd) {
                        if let Some((time, old_val)) = prev_values.get(idx) {
                            if *old_val != val {
                                let app = signal_appearence(&sig, &old_val);
                                let color = app.line_color();
                                let lines = app.line_heights();

                                let y_start = y as f32 * (line_height + padding);
                                for height in lines {
                                    let path = Path::line(
                                        Point::new(
                                            *time as f32,
                                            y_start + (1. - height) * line_height,
                                        ),
                                        Point::new(x as f32, y_start + (1. - height) * line_height),
                                    );
                                    frame.stroke(
                                        &path,
                                        Stroke::default().with_color(color).with_width(1.0),
                                    );

                                    frame.fill_rectangle(
                                        Point::new(x as f32, y_start),
                                        Size::new(1. as f32, line_height),
                                        color,
                                    );
                                }

                                prev_values.insert(*idx, (x, val));
                            }
                        } else {
                            prev_values.insert(*idx, (x, val));
                        }
                    }
                }
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _interaction: &Interaction,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}

impl State {
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
                    }
                    else {
                        None
                    }
                } else {
                    // Scroll 5% of the viewport per scroll event
                    let scroll_step = (&self.viewport.curr_right - &self.viewport.curr_left)
                        / BigInt::from_u32(20).unwrap();

                    let to_scroll =
                        BigRational::from(scroll_step.clone()) * BigRational::from_float(y).unwrap();

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

enum SignalAppearence {
    HighImp,
    Undef,
    False,
    True,
    Wide,
}

impl SignalAppearence {
    fn line_color(&self) -> Color {
        let min = 0.3;
        let max = 1.0;
        match self {
            SignalAppearence::HighImp => Color::from_rgb(max, max, min),
            SignalAppearence::Undef => Color::from_rgb(max, min, min),
            SignalAppearence::False => Color::from_rgb(min, 0.7, min),
            SignalAppearence::True => Color::from_rgb(min, max, min),
            SignalAppearence::Wide => Color::from_rgb(min, max, min),
        }
    }

    fn line_heights(&self) -> &'static [f32] {
        match self {
            SignalAppearence::HighImp => &[0.5],
            SignalAppearence::Undef => &[0.5],
            SignalAppearence::False => &[0.0],
            SignalAppearence::True => &[1.0],
            SignalAppearence::Wide => &[0.0, 1.0],
        }
    }
}

fn signal_appearence(signal: &Signal, val: &SignalValue) -> SignalAppearence {
    match val {
        SignalValue::BigUint(num) => match signal.num_bits() {
            Some(1) => {
                if num == &BigUint::from_u32(0).unwrap() {
                    SignalAppearence::False
                } else {
                    SignalAppearence::True
                }
            }
            _ => SignalAppearence::Wide,
        },
        SignalValue::String(s) => {
            let s_lower = s.to_lowercase();
            if s_lower.contains("z") {
                SignalAppearence::HighImp
            } else if s_lower.contains("x") {
                SignalAppearence::Undef
            } else {
                SignalAppearence::Wide
            }
        }
    }
}
