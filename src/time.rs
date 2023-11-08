use std::fmt;

use eframe::egui;
use num::{BigInt, BigRational, ToPrimitive};

use crate::Message;

pub struct TimeScale {
    pub unit: TimeUnit,
    pub multiplier: Option<u32>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
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
            TimeUnit::None => write!(f, "-"),
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
    let timeunits = vec![
        TimeUnit::FemtoSeconds,
        TimeUnit::PicoSeconds,
        TimeUnit::NanoSeconds,
        TimeUnit::MicroSeconds,
        TimeUnit::MilliSeconds,
        TimeUnit::Seconds,
    ];
    for timeunit in timeunits {
        ui.radio(*wanted_timeunit == timeunit, timeunit.to_string())
            .clicked()
            .then(|| {
                ui.close_menu();
                msgs.push(Message::SetTimeUnit(timeunit));
            });
    }
}

pub fn time_string(time: &BigInt, timescale: &TimeScale, wanted_timeunit: &TimeUnit) -> String {
    let wanted_exponent = wanted_timeunit.exponent();
    let data_exponent = timescale.unit.exponent();
    let exponent_diff = wanted_exponent - data_exponent;
    if exponent_diff >= 0 {
        let precision = exponent_diff as usize;
        format!(
            "{scaledtime:.precision$} {wanted_timeunit}",
            scaledtime = BigRational::new(
                time * timescale.multiplier.unwrap_or(1),
                (BigInt::from(10)).pow(exponent_diff as u32)
            )
            .to_f64()
            .unwrap_or(f64::NAN)
        )
    } else {
        format!(
            "{scaledtime} {wanted_timeunit}",
            scaledtime = time
                * timescale.multiplier.unwrap_or(1)
                * (BigInt::from(10)).pow(-exponent_diff as u32)
        )
    }
}
