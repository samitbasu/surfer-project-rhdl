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
                if s.contains("x") {
                    (format!("UNDEF"), ValueColor::Undef)
                } else if s.contains("z") {
                    (format!("HIGHIMP"), ValueColor::HighImp)
                } else {
                    (
                        u128::from_str_radix(s, 2)
                            .map(|val| format!("{val}"))
                            .unwrap_or(s.clone()),
                        ValueColor::Normal,
                    )
                }
            }
        }
    }
}

pub struct ExtendingBinaryTranslator {}

impl BasicTranslator for ExtendingBinaryTranslator {
    fn name(&self) -> String {
        String::from("Binary (with extension)")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueColor) {
        let (val, color) = match value {
            SignalValue::BigUint(v) => (format!("{v:b}"), ValueColor::Normal),
            SignalValue::String(s) => {
                let val = s.clone();
                if s.contains("x") {
                    (val, ValueColor::Undef)
                } else if s.contains("z") {
                    (val, ValueColor::HighImp)
                } else {
                    (val, ValueColor::Normal)
                }
            }
        };

        // VCD spec'd bit extension
        let extra_bits = if num_bits > val.len() as u64 {
            let extra_count = num_bits - val.len() as u64;
            let extra_value = match val.chars().next() {
                Some('0') => "0",
                Some('1') => "0",
                Some('x') => "x",
                Some('z') => "z",
                // If we got weird characters, this is probably a string, so we don't
                // do the extension
                _ => "",
            };
            extra_value.repeat(extra_count as usize)
        } else {
            String::new()
        };

        (
            format!("{extra_bits}{val}")
                .chars()
                .chunks(4)
                .into_iter()
                .map(|mut c| c.join(""))
                .join(" "),
            color,
        )
    }
}
