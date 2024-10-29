//! Time handling and formatting.
use std::str::FromStr;

use derive_more::Display;
use ecolor::Color32;
use egui::Ui;
use emath::{Align2, Pos2};
use enum_iterator::Sequence;
use epaint::{FontId, Stroke};
use ftr_parser::types::Timescale;
use itertools::Itertools;
use num::{BigInt, BigRational, ToPrimitive};
use pure_rust_locales::{locale_match, Locale};
use serde::{Deserialize, Serialize};
use sys_locale::get_locale;

use crate::config::SurferConfig;
use crate::viewport::Viewport;
use crate::wave_data::WaveData;
use crate::{translation::group_n_chars, view::DrawingContext, Message, State};

#[derive(Serialize, Deserialize)]
pub struct TimeScale {
    pub unit: TimeUnit,
    pub multiplier: Option<u32>,
}

#[derive(Debug, Clone, Copy, Display, Eq, PartialEq, Serialize, Deserialize, Sequence)]
pub enum TimeUnit {
    #[display("fs")]
    FemtoSeconds,

    #[display("ps")]
    PicoSeconds,

    #[display("ns")]
    NanoSeconds,

    #[display("μs")]
    MicroSeconds,

    #[display("ms")]
    MilliSeconds,

    #[display("s")]
    Seconds,

    #[display("No unit")]
    None,

    #[display("Auto")]
    Auto,
}

pub const DEFAULT_TIMELINE_NAME: &str = "Time";
pub const THIN_SPACE: &str = "\u{2009}";

impl From<wellen::TimescaleUnit> for TimeUnit {
    fn from(timescale: wellen::TimescaleUnit) -> Self {
        match timescale {
            wellen::TimescaleUnit::FemtoSeconds => TimeUnit::FemtoSeconds,
            wellen::TimescaleUnit::PicoSeconds => TimeUnit::PicoSeconds,
            wellen::TimescaleUnit::NanoSeconds => TimeUnit::NanoSeconds,
            wellen::TimescaleUnit::MicroSeconds => TimeUnit::MicroSeconds,
            wellen::TimescaleUnit::MilliSeconds => TimeUnit::MilliSeconds,
            wellen::TimescaleUnit::Seconds => TimeUnit::Seconds,
            wellen::TimescaleUnit::Unknown => TimeUnit::None,
        }
    }
}

impl From<ftr_parser::types::Timescale> for TimeUnit {
    fn from(timescale: Timescale) -> Self {
        match timescale {
            Timescale::Fs => TimeUnit::FemtoSeconds,
            Timescale::Ps => TimeUnit::PicoSeconds,
            Timescale::Ns => TimeUnit::NanoSeconds,
            Timescale::Us => TimeUnit::MicroSeconds,
            Timescale::Ms => TimeUnit::MilliSeconds,
            Timescale::S => TimeUnit::Seconds,
            Timescale::Unit => TimeUnit::None,
            Timescale::None => TimeUnit::None,
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
            TimeUnit::Auto => 0,
        }
    }
    fn from_exponent(exponent: i8) -> Self {
        match exponent {
            -15 => TimeUnit::FemtoSeconds,
            -12 => TimeUnit::PicoSeconds,
            -9 => TimeUnit::NanoSeconds,
            -6 => TimeUnit::MicroSeconds,
            -3 => TimeUnit::MilliSeconds,
            0 => TimeUnit::Seconds,
            _ => panic!("Invalid exponent"),
        }
    }
}

/// Create menu for selecting preferred time unit.
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

/// How to format the time stamps
#[derive(Debug, Deserialize, Serialize)]
pub struct TimeFormat {
    /// How to format the numeric part of the time string
    format: TimeStringFormatting,
    /// Insert a space between number and unit
    show_space: bool,
    /// Display time unit
    show_unit: bool,
}

impl Default for TimeFormat {
    fn default() -> Self {
        TimeFormat {
            format: TimeStringFormatting::No,
            show_space: true,
            show_unit: true,
        }
    }
}

impl TimeFormat {
    /// Utility function to get a copy, but with some values changed.
    pub fn get_with_changes(
        &self,
        format: Option<TimeStringFormatting>,
        show_space: Option<bool>,
        show_unit: Option<bool>,
    ) -> Self {
        TimeFormat {
            format: format.unwrap_or(self.format),
            show_space: show_space.unwrap_or(self.show_space),
            show_unit: show_unit.unwrap_or(self.show_unit),
        }
    }
}

pub fn timeformat_menu(ui: &mut Ui, msgs: &mut Vec<Message>, current_timeformat: &TimeFormat) {
    for time_string_format in enum_iterator::all::<TimeStringFormatting>() {
        ui.radio(
            current_timeformat.format == time_string_format,
            if time_string_format == TimeStringFormatting::Locale {
                format!(
                    "{time_string_format} ({locale})",
                    locale = get_locale().unwrap_or_else(|| "unknown".to_string())
                )
            } else {
                time_string_format.to_string()
            },
        )
        .clicked()
        .then(|| {
            ui.close_menu();
            msgs.push(Message::SetTimeStringFormatting(Some(time_string_format)));
        });
    }
}

/// How to format the numeric part of the time string
#[derive(Debug, Clone, Copy, Display, Eq, PartialEq, Serialize, Deserialize, Sequence)]
pub enum TimeStringFormatting {
    /// No additional formatting
    #[display("No")]
    No,

    /// Use the current locale to determine decimal separator, thousands separator, and grouping
    #[display("Locale")]
    Locale,

    /// Use the SI standard: split into groups of three digits, unless there are exactly four
    /// for both integer and fractional part. Use space as group separator.
    #[display("SI")]
    SI,
}

impl FromStr for TimeStringFormatting {
    type Err = String;

    fn from_str(input: &str) -> Result<TimeStringFormatting, Self::Err> {
        match input {
            "No" => Ok(TimeStringFormatting::No),
            "Locale" => Ok(TimeStringFormatting::Locale),
            "SI" => Ok(TimeStringFormatting::SI),
            _ => Err(format!(
                "'{input}' is not a valid TimeFormat (Valid options: No|Locale|SI)"
            )),
        }
    }
}

/// Get rid of trailing zeros if the string contains a ., i.e., being fractional
/// If the resulting string ends with ., remove that as well.
fn strip_trailing_zeros_and_period(time: String) -> String {
    if time.contains('.') {
        time.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        time
    }
}

/// Format number based on [`TimeStringFormatting`], i.e., possibly group digits together
/// and use correct separator for each group.
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
                // "\u{202f}" (non-breaking thin space) does not exist in used font, replace with "\u{2009}" (thin space)
                let thousands_sep = locale_match!(locale => LC_NUMERIC::THOUSANDS_SEP)
                    .replace('\u{202f}', THIN_SPACE);
                if time.contains('.') {
                    let decimal_point = locale_match!(locale => LC_NUMERIC::DECIMAL_POINT);
                    let mut parts = time.split('.');
                    let integer_result = group_n_chars(parts.next().unwrap(), grouping[0] as usize)
                        .join(thousands_sep.as_str());
                    let fractional_part = parts.next().unwrap();
                    format!("{integer_result}{decimal_point}{fractional_part}")
                } else {
                    group_n_chars(&time, grouping[0] as usize).join(thousands_sep.as_str())
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
                    group_n_chars(integer_part, 3).join(THIN_SPACE)
                } else {
                    integer_part.to_string()
                };
                if fractional_part.len() > 4 {
                    let reversed = fractional_part.chars().rev().collect::<String>();
                    let reversed_fractional_parts = group_n_chars(&reversed, 3).join(THIN_SPACE);
                    let fractional_result =
                        reversed_fractional_parts.chars().rev().collect::<String>();
                    format!("{integer_result}.{fractional_result}")
                } else {
                    format!("{integer_result}.{fractional_part}")
                }
            } else if time.len() > 4 {
                group_n_chars(&time, 3).join(THIN_SPACE)
            } else {
                time
            }
        }
    }
}

/// Heuristically find a suitable time unit for the given time.
fn find_auto_scale(time: &BigInt, timescale: &TimeScale) -> TimeUnit {
    // In case of seconds, nothing to do as it is the largest supported unit
    // (unless we want to support minutes etc...)
    if timescale.unit == TimeUnit::Seconds {
        return TimeUnit::Seconds;
    }
    let multiplier_digits = timescale.multiplier.unwrap_or(1).ilog10();
    let start_digits = -timescale.unit.exponent();
    for e in (3..=start_digits).step_by(3).rev() {
        if (time % (10_u32.pow(e as u32 - multiplier_digits))) == BigInt::from(0) {
            return TimeUnit::from_exponent(e - start_digits);
        }
    }
    timescale.unit
}

/// Format the time string taking all settings into account.
pub fn time_string(
    time: &BigInt,
    timescale: &TimeScale,
    wanted_timeunit: &TimeUnit,
    wanted_time_format: &TimeFormat,
) -> String {
    if wanted_timeunit == &TimeUnit::Auto {
        let auto_timeunit = find_auto_scale(time, timescale);
        return time_string(time, timescale, &auto_timeunit, wanted_time_format);
    }
    if wanted_timeunit == &TimeUnit::None {
        return split_and_format_number(time.to_string(), &wanted_time_format.format);
    }
    let wanted_exponent = wanted_timeunit.exponent();
    let data_exponent = timescale.unit.exponent();
    let exponent_diff = wanted_exponent - data_exponent;
    let timeunit = if wanted_time_format.show_unit {
        wanted_timeunit.to_string()
    } else {
        String::new()
    };
    let space = if wanted_time_format.show_space {
        " ".to_string()
    } else {
        String::new()
    };
    let timestring = if exponent_diff >= 0 {
        let precision = exponent_diff as usize;
        strip_trailing_zeros_and_period(format!(
            "{scaledtime:.precision$}",
            scaledtime = BigRational::new(
                time * timescale.multiplier.unwrap_or(1),
                (BigInt::from(10)).pow(exponent_diff as u32)
            )
            .to_f64()
            .unwrap_or(f64::NAN)
        ))
    } else {
        (time * timescale.multiplier.unwrap_or(1) * (BigInt::from(10)).pow(-exponent_diff as u32))
            .to_string()
    };
    format!(
        "{scaledtime}{space}{timeunit}",
        scaledtime = split_and_format_number(timestring, &wanted_time_format.format)
    )
}

impl WaveData {
    /// Get suitable tick locations for the current view port.
    /// The method is based on guessing the length of the time string and
    /// is inspired by the corresponding code in Matplotlib.
    pub fn get_ticks(
        &self,
        viewport: &Viewport,
        timescale: &TimeScale,
        frame_width: f32,
        text_size: f32,
        wanted_timeunit: &TimeUnit,
        time_format: &TimeFormat,
        config: &SurferConfig,
    ) -> Vec<(String, f32)> {
        let char_width = text_size * (20. / 31.);
        let rightexp = viewport
            .curr_right
            .absolute(&self.num_timestamps())
            .0
            .abs()
            .log10()
            .round() as i16;
        let leftexp = viewport
            .curr_left
            .absolute(&self.num_timestamps())
            .0
            .abs()
            .log10()
            .round() as i16;
        let max_labelwidth = (rightexp.max(leftexp) + 3) as f32 * char_width;
        let max_labels = ((frame_width * config.theme.ticks.density) / max_labelwidth).floor() + 2.;
        let scale = 10.0f64.powf(
            ((viewport.curr_right - viewport.curr_left)
                .absolute(&self.num_timestamps())
                .0
                / max_labels as f64)
                .log10()
                .floor(),
        );

        let steps = &[1., 2., 2.5, 5., 10., 20., 25., 50.];
        let mut ticks: Vec<(String, f32)> = [].to_vec();
        for step in steps {
            let scaled_step = scale * step;
            let rounded_min_label_time =
                (viewport.curr_left.absolute(&self.num_timestamps()).0 / scaled_step).floor()
                    * scaled_step;
            let high = ((viewport.curr_right.absolute(&self.num_timestamps()).0
                - rounded_min_label_time)
                / scaled_step)
                .ceil() as f32
                + 1.;
            if high <= max_labels {
                ticks = (0..high as i16)
                    .map(|v| {
                        BigInt::from(((v as f64) * scaled_step + rounded_min_label_time) as i128)
                    })
                    .unique()
                    .map(|tick| {
                        (
                            // Time string
                            time_string(&tick, timescale, wanted_timeunit, time_format),
                            viewport.pixel_from_time(&tick, frame_width, &self.num_timestamps()),
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

    /// Draw the text for each tick location.
    pub fn draw_ticks(
        &self,
        color: Option<&Color32>,
        ticks: &Vec<(String, f32)>,
        ctx: &DrawingContext<'_>,
        y_offset: f32,
        align: Align2,
        config: &SurferConfig,
    ) {
        let color = *color.unwrap_or(&config.theme.foreground);

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

impl State {
    pub fn get_time_format(&self) -> TimeFormat {
        self.config
            .default_time_format
            .get_with_changes(self.time_string_format, None, None)
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
                    show_space: false,
                    show_unit: true
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
                    show_space: false,
                    show_unit: false
                }
            ),
            "22000"
        );
    }
    #[test]
    fn print_time_si() {
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
                    show_space: true,
                    show_unit: true
                }
            ),
            "123\u{2009}456.789\u{2009}01 s"
        );
        assert_eq!(
            time_string(
                &BigInt::from(1456789100i128),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Seconds,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "1456.7891 s"
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
                    show_space: true,
                    show_unit: true
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
                    show_space: true,
                    show_unit: true
                }
            ),
            "22\u{2009}200 μs"
        );
    }
    #[test]
    fn print_time_auto() {
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "2200 μs"
        );
        assert_eq!(
            time_string(
                &BigInt::from(22000),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "22 ms"
        );
        assert_eq!(
            time_string(
                &BigInt::from(22000),
                &TimeScale {
                    multiplier: Some(10),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "220 ms"
        );
        assert_eq!(
            time_string(
                &BigInt::from(220000),
                &TimeScale {
                    multiplier: Some(100),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "22 s"
        );
        assert_eq!(
            time_string(
                &BigInt::from(22000),
                &TimeScale {
                    multiplier: Some(10),
                    unit: TimeUnit::Seconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: true,
                    show_unit: true
                }
            ),
            "220000 s"
        );
    }
    #[test]
    fn print_time_none() {
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::None,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: true,
                    show_unit: true
                }
            ),
            "2200"
        );
        assert_eq!(
            time_string(
                &BigInt::from(220),
                &TimeScale {
                    multiplier: Some(10),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::None,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: true,
                    show_unit: true
                }
            ),
            "220"
        );
    }
}
