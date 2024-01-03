use std::{fmt, str::FromStr};

use eframe::{
    egui,
    epaint::{Pos2, Stroke},
};
use enum_iterator::Sequence;
use num::{BigInt, BigRational, ToPrimitive};
use pure_rust_locales::{locale_match, Locale};
use serde::{Deserialize, Serialize};
use sys_locale::get_locale;

use crate::{
    translation::group_n_chars, view::DrawingContext, wave_data::WaveData, Message, State,
};

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

pub fn timeunit_menu(ui: &mut egui::Ui, msgs: &mut Vec<Message>, wanted_timeunit: &TimeUnit) {
    for timeunit in enum_iterator::all::<TimeUnit>() {
        ui.radio(*wanted_timeunit == timeunit, timeunit.to_string())
            .clicked()
            .then(|| {
                ui.close_menu();
                msgs.push(Message::SetTimeUnit(timeunit));
            });
    }
}

#[derive(Debug, Deserialize)]
pub struct TimeFormat {
    format: TimeStringFormatting,
    space: bool,
    unit: bool,
}

impl Default for TimeFormat {
    fn default() -> Self {
        TimeFormat {
            format: TimeStringFormatting::No,
            space: true,
            unit: true,
        }
    }
}

/// How to format the numeric part of the time string
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Sequence)]
pub enum TimeStringFormatting {
    /// No additional formatting
    No,
    /// Use the current locale to determine decimal separator, thousands separator, and grouping
    Locale,
    /// Use the SI standard: split into groups of three digits, unless there are exactly four
    /// for both integer and fractional part. Use space as group separator.
    SI,
}

impl fmt::Display for TimeStringFormatting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeStringFormatting::No => write!(f, "No"),
            TimeStringFormatting::Locale => write!(f, "Locale"),
            TimeStringFormatting::SI => write!(f, "SI"),
        }
    }
}

impl FromStr for TimeStringFormatting {
    type Err = String;

    fn from_str(input: &str) -> Result<TimeStringFormatting, Self::Err> {
        match input {
            "No" => Ok(TimeStringFormatting::No),
            "Locale" => Ok(TimeStringFormatting::Locale),
            "SI" => Ok(TimeStringFormatting::SI),
            _ => Err(format!(
                "'{}' is not a valid TimeFormat (Valid options: No|Locale|SI)",
                input
            )),
        }
    }
}

fn strip_trailing_zeros_and_period(time: String) -> String {
    if time.contains('.') {
        time.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        time
    }
}

fn split_and_format_number(time: String, format: &TimeStringFormatting) -> String {
    match format {
        TimeStringFormatting::No => time,
        TimeStringFormatting::Locale => {
            let locale: Locale = get_locale()
                .unwrap_or_else(|| "en-US".to_string())
                .as_str()
                .try_into()
                .unwrap_or(Locale::en_US);
            let grouping = locale_match!(locale => LC_NUMERIC::GROUPING);
            if grouping[0] > 0 {
                let thousands_sep = locale_match!(locale => LC_NUMERIC::THOUSANDS_SEP);
                if time.contains('.') {
                    let decimal_point = locale_match!(locale => LC_NUMERIC::DECIMAL_POINT);
                    let mut parts = time.split('.');
                    let integer_result = group_n_chars(parts.next().unwrap(), grouping[0] as usize)
                        .join(thousands_sep);
                    let fractional_part = parts.next().unwrap();
                    format!("{integer_result}{decimal_point}{fractional_part}")
                } else {
                    group_n_chars(&time, grouping[0] as usize).join(thousands_sep)
                }
            } else {
                time
            }
        }
        TimeStringFormatting::SI => {
            if time.contains('.') {
                let mut parts = time.split('.');
                let integer_part = parts.next().unwrap();
                let fractional_part = parts.next().unwrap();
                let integer_result = if integer_part.len() > 4 {
                    group_n_chars(integer_part, 3).join(" ")
                } else {
                    integer_part.to_string()
                };
                if fractional_part.len() > 4 {
                    let reversed = fractional_part.chars().rev().collect::<String>();
                    let reversed_fractional_parts = group_n_chars(&reversed, 3).join(" ");
                    let fractional_result =
                        reversed_fractional_parts.chars().rev().collect::<String>();
                    format!("{integer_result}.{fractional_result}")
                } else {
                    format!("{integer_result}.{fractional_part}")
                }
            } else {
                if time.len() > 4 {
                    group_n_chars(&time, 3).join(" ")
                } else {
                    time
                }
            }
        }
    }
}

pub fn time_string(
    time: &BigInt,
    timescale: &TimeScale,
    wanted_timeunit: &TimeUnit,
    wanted_time_format: &TimeFormat,
) -> String {
    if wanted_timeunit == &TimeUnit::None {
        return split_and_format_number(time.to_string(), &wanted_time_format.format);
    }
    let wanted_exponent = wanted_timeunit.exponent();
    let data_exponent = timescale.unit.exponent();
    let exponent_diff = wanted_exponent - data_exponent;
    let timeunit = if wanted_time_format.unit {
        wanted_timeunit.to_string()
    } else {
        String::new()
    };
    let space = if wanted_time_format.space {
        " ".to_string()
    } else {
        String::new()
    };
    if exponent_diff >= 0 {
        let precision = exponent_diff as usize;
        let scaledtime = split_and_format_number(
            strip_trailing_zeros_and_period(format!(
                "{scaledtime:.precision$}",
                scaledtime = BigRational::new(
                    time * timescale.multiplier.unwrap_or(1),
                    (BigInt::from(10)).pow(exponent_diff as u32)
                )
                .to_f64()
                .unwrap_or(f64::NAN)
            )),
            &wanted_time_format.format,
        );

        format!("{scaledtime}{space}{timeunit}")
    } else {
        format!(
            "{scaledtime}{space}{timeunit}",
            scaledtime = split_and_format_number(
                (time
                    * timescale.multiplier.unwrap_or(1)
                    * (BigInt::from(10)).pow(-exponent_diff as u32))
                .to_string(),
                &wanted_time_format.format
            )
        )
    }
}

impl State {
    pub fn get_ticks(
        &self,
        waves: &WaveData,
        frame_width: f32,
        text_size: f32,
    ) -> Vec<(String, f32)> {
        let char_width = text_size * (20. / 31.);
        let left_time = waves.viewport.to_time_f64(0., frame_width);
        let frame_width_64 = frame_width as f64;
        let right_time = waves.viewport.to_time_f64(frame_width_64, frame_width);
        let time_width = right_time - left_time;
        let rightexp = right_time.abs().log10().round() as i16;
        let leftexp = left_time.abs().log10().round() as i16;
        let max_labelwidth = (rightexp.max(leftexp) + 3) as f32 * char_width;
        let max_labels = ((frame_width * self.config.ticks.density) / max_labelwidth).floor() + 2.;
        let scale = 10.0f64.powf((time_width / max_labels as f64).log10().floor());

        let steps = &[1., 2., 2.5, 5., 10.];
        let mut ticks: Vec<(String, f32)> = [].to_vec();
        for step in steps {
            let scaled_step = scale * step;
            if (scaled_step.round() - scaled_step).abs() >= 0.1 {
                // Do not select a step size so that we get ticks that are drawn inbetween
                // possible cursor positions
                continue;
            }
            let rounded_min_label_time = (left_time / scaled_step).floor() * scaled_step;
            let high = ((right_time - rounded_min_label_time) / scaled_step).ceil() as f32 + 1.;
            if high <= max_labels {
                ticks = (0..high as i16)
                    .map(|v| {
                        BigInt::from(((v as f64) * scaled_step + rounded_min_label_time) as i128)
                    })
                    .map(|tick| {
                        (
                            // Time string
                            time_string(
                                &tick,
                                &waves.inner.metadata().timescale,
                                &self.wanted_timeunit,
                                &self.config.default_time_format,
                            ),
                            waves.viewport.from_time(&tick, frame_width_64) as f32,
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
}

#[cfg(test)]
mod test {
    use num::BigInt;

    use crate::time::{time_string, TimeFormat, TimeScale, TimeStringFormatting, TimeUnit};

    #[test]
    fn print_time_standard() {
        assert_eq!(
            time_string(
                &BigInt::from(103),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::FemtoSeconds
                },
                &TimeUnit::FemtoSeconds,
                &TimeFormat::default()
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
                &TimeUnit::MicroSeconds,
                &TimeFormat::default()
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
                &TimeUnit::MilliSeconds,
                &TimeFormat::default()
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
                &TimeUnit::NanoSeconds,
                &TimeFormat::default()
            ),
            "2200000 ns"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::NanoSeconds
                },
                &TimeUnit::PicoSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    space: false,
                    unit: true
                }
            ),
            "2200000ps"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(10),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MicroSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    space: false,
                    unit: false
                }
            ),
            "22000"
        );
        assert_eq!(
            time_string(
                &BigInt::from(123456789010i128),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Seconds,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    space: true,
                    unit: true
                }
            ),
            "123 456.789 01 s"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MicroSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    space: true,
                    unit: true
                }
            ),
            "2200 μs"
        );
        assert_eq!(
            time_string(
                &BigInt::from(22200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MicroSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    space: true,
                    unit: true
                }
            ),
            "22 200 μs"
        );
    }
}
