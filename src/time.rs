use std::fmt;

use eframe::egui;
use fastwave_backend::{self, Metadata};
use num::{BigInt, BigRational, ToPrimitive};

use crate::Message;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TimeScale {
    FemtoSeconds,
    PicoSeconds,
    NanoSeconds,
    MicroSeconds,
    MilliSeconds,
    Seconds,
    None,
}

impl fmt::Display for TimeScale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeScale::FemtoSeconds => write!(f, "fs"),
            TimeScale::PicoSeconds => write!(f, "ps"),
            TimeScale::NanoSeconds => write!(f, "ns"),
            TimeScale::MicroSeconds => write!(f, "μs"),
            TimeScale::MilliSeconds => write!(f, "ms"),
            TimeScale::Seconds => write!(f, "s"),
            TimeScale::None => write!(f, "-"),
        }
    }
}

impl From<fastwave_backend::Timescale> for TimeScale {
    fn from(timescale: fastwave_backend::Timescale) -> Self {
        match timescale {
            fastwave_backend::Timescale::Fs => TimeScale::FemtoSeconds,
            fastwave_backend::Timescale::Ps => TimeScale::PicoSeconds,
            fastwave_backend::Timescale::Ns => TimeScale::NanoSeconds,
            fastwave_backend::Timescale::Us => TimeScale::MicroSeconds,
            fastwave_backend::Timescale::Ms => TimeScale::MilliSeconds,
            fastwave_backend::Timescale::S => TimeScale::Seconds,
            fastwave_backend::Timescale::Unit => TimeScale::None,
        }
    }
}

impl TimeScale {
    fn exponent(&self) -> i8 {
        match self {
            TimeScale::FemtoSeconds => -15,
            TimeScale::PicoSeconds => -12,
            TimeScale::NanoSeconds => -9,
            TimeScale::MicroSeconds => -6,
            TimeScale::MilliSeconds => -3,
            TimeScale::Seconds => 0,
            TimeScale::None => 0,
        }
    }
}

pub fn timescale_menu(ui: &mut egui::Ui, msgs: &mut Vec<Message>, wanted_timescale: &TimeScale) {
    let timescales = vec![
        TimeScale::FemtoSeconds,
        TimeScale::PicoSeconds,
        TimeScale::NanoSeconds,
        TimeScale::MicroSeconds,
        TimeScale::MilliSeconds,
        TimeScale::Seconds,
    ];
    for timescale in timescales {
        ui.radio(*wanted_timescale == timescale, timescale.to_string())
            .clicked()
            .then(|| {
                ui.close_menu();
                msgs.push(Message::SetTimeScale(timescale));
            });
    }
}

pub fn time_string(time: &BigInt, metadata: &Metadata, wanted_timescale: &TimeScale) -> String {
    let wanted_exponent = wanted_timescale.exponent();
    let data_exponent = TimeScale::from(metadata.timescale.1).exponent();
    let exponent_diff = wanted_exponent - data_exponent;
    if exponent_diff >= 0 {
        let precision = exponent_diff as usize;
        format!(
            "{scaledtime:.precision$} {wanted_timescale}",
            scaledtime = BigRational::new(
                time * metadata.timescale.0.unwrap_or(1),
                (BigInt::from(10)).pow(exponent_diff as u32)
            )
            .to_f64()
            .unwrap_or(f64::NAN)
        )
    } else {
        format!(
            "{scaledtime} {wanted_timescale}",
            scaledtime = time
                * metadata.timescale.0.unwrap_or(1)
                * (BigInt::from(10)).pow(-exponent_diff as u32)
        )
    }
}
