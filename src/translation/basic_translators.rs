use std::collections::HashMap;

use super::{BasicTranslator, SignalInfo, TranslationResult, Translator, ValueRepr};

use color_eyre::Result;
use fastwave_backend::{Signal, SignalValue};
use itertools::Itertools;

pub struct HexTranslator {}

impl BasicTranslator for HexTranslator {
    fn name(&self) -> String {
        String::from("Hexadecimal")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> String {
        match value {
            SignalValue::BigUint(v) => {
                format!("{v:0width$x}", width = num_bits as usize / 4)
            }
            SignalValue::String(s) => s
                .chars()
                .chunks(4)
                .into_iter()
                .map(|c| {
                    let c = c.collect::<String>();
                    if c.contains('x') {
                        "x".to_string()
                    } else if c.contains('z') {
                        "z".to_string()
                    } else {
                        format!(
                            "{:x}",
                            u8::from_str_radix(&c, 2)
                                .expect("Found non binary digit in value")
                        )
                    }
                })
                .join(""),
        }
    }
}

pub struct UnsignedTranslator {}

impl BasicTranslator for UnsignedTranslator {
    fn name(&self) -> String {
        String::from("Unsigned")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> String {
        match value {
            SignalValue::BigUint(v) => format!("{v}"),
            SignalValue::String(s) => s.clone(),
        }
    }
}
