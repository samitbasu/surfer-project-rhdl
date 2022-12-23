use std::collections::{BTreeMap, HashMap};

use fastwave_backend::{Signal, SignalIdx, SignalValue, VCD};
use iced::mouse::Interaction;
use iced::widget::canvas::event::{self, Event};
use iced::widget::canvas::{self, Frame, Stroke};
use iced::widget::canvas::{Cache, Cursor, Geometry, Path};
use iced::{mouse, Color, Point, Rectangle, Size, Theme, Vector};
use num::BigUint;
use num::FromPrimitive;

use crate::{Message, State};

impl<'a> canvas::Program<Message> for State {
    type State = Interaction;

    fn update(
        &self,
        _interaction: &mut Interaction,
        _event: Event,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> (event::Status, Option<Message>) {
        (event::Status::Ignored, None)
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
                let min = num::BigUint::from_u64(0).unwrap();
                let max = num::BigUint::from_u64(671_000).unwrap();

                let time_spacing = max / BigUint::from_u64(frame.width() as u64).unwrap();

                let time = time_spacing * x;

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
                                        Stroke::default().with_color(color)
                                        .with_width(1.0)
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
