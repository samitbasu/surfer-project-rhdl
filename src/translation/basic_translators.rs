use super::{BasicTranslator, ValueColor};

use fastwave_backend::SignalValue;
use itertools::Itertools;

pub struct HexTranslator {}

impl BasicTranslator for HexTranslator {
    fn name(&self) -> String {
        String::from("Hexadecimal")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueColor) {
        match value {
            SignalValue::BigUint(v) => (
                format!("{v:0width$x}", width = num_bits as usize / 4),
                ValueColor::Normal,
            ),
            SignalValue::String(s) => {
                let mut is_undef = false;
                let mut is_highimp = false;
                let val = s
                    .chars()
                    .chunks(4)
                    .into_iter()
                    .map(|c| {
                        let c = c.collect::<String>();
                        if c.contains('x') {
                            is_undef = true;
                            "x".to_string()
                        } else if c.contains('z') {
                            is_highimp = true;
                            "z".to_string()
                        } else {
                            format!(
                                "{:x}",
                                u8::from_str_radix(&c, 2).expect("Found non binary digit in value")
                            )
                        }
                    })
                    .join("");

                (
                    val,
                    if is_undef {
                        ValueColor::Undef
                    } else if is_highimp {
                        ValueColor::HighImp
                    } else {
                        ValueColor::Normal
                    },
                )
            }
        }
    }
}

pub struct UnsignedTranslator {}

impl BasicTranslator for UnsignedTranslator {
    fn name(&self) -> String {
        String::from("Unsigned")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueColor) {
        match value {
            SignalValue::BigUint(v) => (format!("{v}"), ValueColor::Normal),
            SignalValue::String(s) => {
                let color = if s.contains("x") {
                    ValueColor::Undef
                } else if s.contains("z") {
                    ValueColor::HighImp
                } else {
                    ValueColor::Normal
                };
                (format!("0b{}", s), color)
            }
        }
    }
}
