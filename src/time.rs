use eframe::egui;
use fastwave_backend::{Metadata, Timescale};
use num::{BigInt, BigRational, ToPrimitive};

use crate::Message;

pub fn timescale_menu(ui: &mut egui::Ui, msgs: &mut Vec<Message>, wanted_timescale: &Timescale) {
    let timescales = vec![
        Timescale::Fs,
        Timescale::Ps,
        Timescale::Ns,
        Timescale::Us,
        Timescale::Ms,
        Timescale::S,
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

fn strip_trailing_zeros_and_period(time: String) -> String {
    if time.contains('.') {
        time.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        time
    }
}

pub fn time_string(time: &BigInt, metadata: &Metadata, wanted_timescale: &Timescale) -> String {
    let wanted_exponent = timescale_to_exponent(wanted_timescale);
    let data_exponent = timescale_to_exponent(&metadata.timescale.1);
    let exponent_diff = wanted_exponent - data_exponent;
    if exponent_diff >= 0 {
        let precision = exponent_diff as usize;
        let scaledtime = strip_trailing_zeros_and_period(format!(
            "{scaledtime:.precision$}",
            scaledtime = BigRational::new(
                time * metadata.timescale.0.unwrap_or(1),
                (BigInt::from(10)).pow(exponent_diff as u32)
            )
            .to_f64()
            .unwrap_or(f64::NAN)
        ));

        format!("{scaledtime} {wanted_timescale}")
    } else {
        format!(
            "{scaledtime} {wanted_timescale}",
            scaledtime = time
                * metadata.timescale.0.unwrap_or(1)
                * (BigInt::from(10)).pow(-exponent_diff as u32)
        )
    }
}

fn timescale_to_exponent(timescale: &Timescale) -> i8 {
    match timescale {
        Timescale::Fs => -15,
        Timescale::Ps => -12,
        Timescale::Ns => -9,
        Timescale::Us => -6,
        Timescale::Ms => -3,
        Timescale::S => 0,
        _ => 0,
    }
}
