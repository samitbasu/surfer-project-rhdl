use std::collections::{BTreeMap, HashMap};

use fastwave_backend::{SignalIdx, VCD};
use iced::mouse::Interaction;
use iced::widget::canvas::event::{self, Event};
use iced::widget::canvas::{self, Frame};
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
                let max = num::BigUint::from_u64(41_700_000).unwrap();

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
                    match sig.query_num_val_on_tmln(&time, &vcd) {
                        Ok(val) => {
                            let digits = val.to_u64_digits();
                            let val = if digits.is_empty() {
                                Some(0)
                            } else {
                                Some(digits[0])
                            };

                            if let Some((time, old_val)) = prev_values.get(idx) {
                                if *old_val != val {
                                    let (height, color) = match old_val {
                                        Some(v) => {
                                            if *v == 0 {
                                                (line_height, Color::from_rgb8(60, 255, 60))
                                            } else {
                                                (0., Color::from_rgb8(60, 60, 255))
                                            }
                                        }
                                        None => (0.5 * line_height, Color::from_rgb8(255, 255, 60)),
                                    };

                                    let y_start = y as f32 * (line_height + padding);
                                    frame.fill_rectangle(
                                        Point::new(*time as f32, y_start + height),
                                        Size::new((x - time) as f32, 2.),
                                        color,
                                    );

                                    frame.fill_rectangle(
                                        Point::new(x as f32, y_start),
                                        Size::new(1. as f32, line_height),
                                        color,
                                    );

                                    prev_values.insert(*idx, (x, val));
                                }
                            }
                            else {
                                prev_values.insert(*idx, (x, val));
                            }
                        }
                        // TODO: Do not just ignore this
                        Err(_) => {},
                    };
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
