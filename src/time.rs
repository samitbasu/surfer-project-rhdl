use std::fmt;

use eframe::egui::Ui;
use eframe::emath::{Align2, Pos2};
use eframe::epaint::{Color32, FontId, Stroke};
use enum_iterator::Sequence;
use num::{BigInt, BigRational, ToPrimitive};
use serde::{Deserialize, Serialize};

use crate::{view::DrawingContext, viewport::Viewport, Message, State};

#[derive(Serialize, Deserialize)]
pub struct TimeScale {
    pub unit: TimeUnit,
    pub multiplier: Option<u32>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Sequence)]
pub enum TimeUnit {
    FemtoSeconds,
    PicoSeconds,
    NanoSeconds,
    MicroSeconds,
    MilliSeconds,
    Seconds,
    None,
}

pub const DEFAULT_TIMELINE_NAME: &str = "Time";

impl fmt::Display for TimeUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeUnit::FemtoSeconds => write!(f, "fs"),
            TimeUnit::PicoSeconds => write!(f, "ps"),
            TimeUnit::NanoSeconds => write!(f, "ns"),
            TimeUnit::MicroSeconds => write!(f, "μs"),
            TimeUnit::MilliSeconds => write!(f, "ms"),
            TimeUnit::Seconds => write!(f, "s"),
            TimeUnit::None => write!(f, "No unit"),
        }
    }
}

impl From<fastwave_backend::Timescale> for TimeUnit {
    fn from(timescale: fastwave_backend::Timescale) -> Self {
        match timescale {
            fastwave_backend::Timescale::Fs => TimeUnit::FemtoSeconds,
            fastwave_backend::Timescale::Ps => TimeUnit::PicoSeconds,
            fastwave_backend::Timescale::Ns => TimeUnit::NanoSeconds,
            fastwave_backend::Timescale::Us => TimeUnit::MicroSeconds,
            fastwave_backend::Timescale::Ms => TimeUnit::MilliSeconds,
            fastwave_backend::Timescale::S => TimeUnit::Seconds,
            fastwave_backend::Timescale::Unit => TimeUnit::None,
        }
    }
}

impl TimeUnit {
    fn exponent(&self) -> i8 {
        match self {
            TimeUnit::FemtoSeconds => -15,
            TimeUnit::PicoSeconds => -12,
            TimeUnit::NanoSeconds => -9,
            TimeUnit::MicroSeconds => -6,
            TimeUnit::MilliSeconds => -3,
            TimeUnit::Seconds => 0,
            TimeUnit::None => 0,
        }
    }
}

pub fn timeunit_menu(ui: &mut Ui, msgs: &mut Vec<Message>, wanted_timeunit: &TimeUnit) {
    for timeunit in enum_iterator::all::<TimeUnit>() {
        ui.radio(*wanted_timeunit == timeunit, timeunit.to_string())
            .clicked()
            .then(|| {
                ui.close_menu();
                msgs.push(Message::SetTimeUnit(timeunit));
            });
    }
}

fn strip_trailing_zeros_and_period(time: String) -> String {
    if time.contains('.') {
        time.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        time
    }
}

pub fn time_string(time: &BigInt, timescale: &TimeScale, wanted_timeunit: &TimeUnit) -> String {
    if wanted_timeunit == &TimeUnit::None {
        return time.to_string();
    }
    let wanted_exponent = wanted_timeunit.exponent();
    let data_exponent = timescale.unit.exponent();
    let exponent_diff = wanted_exponent - data_exponent;
    if exponent_diff >= 0 {
        let precision = exponent_diff as usize;
        let scaledtime = strip_trailing_zeros_and_period(format!(
            "{scaledtime:.precision$}",
            scaledtime = BigRational::new(
                time * timescale.multiplier.unwrap_or(1),
                (BigInt::from(10)).pow(exponent_diff as u32)
            )
            .to_f64()
            .unwrap_or(f64::NAN)
        ));

        format!("{scaledtime} {wanted_timeunit}")
    } else {
        format!(
            "{scaledtime} {wanted_timeunit}",
            scaledtime = time
                * timescale.multiplier.unwrap_or(1)
                * (BigInt::from(10)).pow(-exponent_diff as u32)
        )
    }
}

impl State {
    pub fn get_ticks(
        &self,
        viewport: &Viewport,
        timescale: &TimeScale,
        frame_width: f32,
        text_size: f32,
    ) -> Vec<(String, f32)> {
        let char_width = text_size * (20. / 31.);
        let rightexp = viewport.curr_right.abs().log10().round() as i16;
        let leftexp = viewport.curr_left.abs().log10().round() as i16;
        let max_labelwidth = (rightexp.max(leftexp) + 3) as f32 * char_width;
        let max_labels = ((frame_width * self.config.ticks.density) / max_labelwidth).floor() + 2.;
        let scale = 10.0f64.powf(
            ((viewport.curr_right - viewport.curr_left) / max_labels as f64)
                .log10()
                .floor(),
        );

        let steps = &[1., 2., 2.5, 5., 10.];
        let mut ticks: Vec<(String, f32)> = [].to_vec();
        for step in steps {
            let scaled_step = scale * step;
            if (scaled_step.round() - scaled_step).abs() >= 0.1 {
                // Do not select a step size so that we get ticks that are drawn inbetween
                // possible cursor positions
                continue;
            }
            let rounded_min_label_time = (viewport.curr_left / scaled_step).floor() * scaled_step;
            let high =
                ((viewport.curr_right - rounded_min_label_time) / scaled_step).ceil() as f32 + 1.;
            if high <= max_labels {
                ticks = (0..high as i16)
                    .map(|v| {
                        BigInt::from(((v as f64) * scaled_step + rounded_min_label_time) as i128)
                    })
                    .map(|tick| {
                        (
                            // Time string
                            time_string(&tick, timescale, &self.wanted_timeunit),
                            viewport.from_time(&tick, frame_width as f64) as f32,
                        )
                    })
                    .collect::<Vec<(String, f32)>>();
                break;
            }
        }
        ticks
    }

    pub fn draw_tick_line(&self, x: f32, ctx: &mut DrawingContext, stroke: &Stroke) {
        let Pos2 {
            x: x_pos,
            y: y_start,
        } = (ctx.to_screen)(x, 0.);
        ctx.painter.vline(
            x_pos,
            (y_start)..=(y_start + ctx.cfg.canvas_height),
            *stroke,
        );
    }

    pub fn draw_ticks(
        &self,
        color: Option<&Color32>,
        ticks: &Vec<(String, f32)>,
        ctx: &DrawingContext<'_>,
        y_offset: f32,
        align: Align2,
    ) {
        let color = *color.unwrap_or(&self.config.theme.foreground);

        for (tick_text, x) in ticks {
            ctx.painter.text(
                (ctx.to_screen)(*x, y_offset),
                align,
                tick_text,
                FontId::proportional(ctx.cfg.text_size),
                color,
            );
        }
    }
}

#[cfg(test)]
mod test {
    use num::BigInt;

    use crate::time::{time_string, TimeScale, TimeUnit};

    #[test]
    fn print_time_standard() {
        assert_eq!(
            time_string(
                &BigInt::from(103),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::FemtoSeconds
                },
                &TimeUnit::FemtoSeconds
            ),
            "103 fs"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MicroSeconds
            ),
            "2200 μs"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MilliSeconds
            ),
            "2.2 ms"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::NanoSeconds
            ),
            "2200000 ns"
        );
    }
}
