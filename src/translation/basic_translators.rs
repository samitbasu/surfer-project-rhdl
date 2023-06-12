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

pub struct HierarchicalTranslator {}

impl Translator for HierarchicalTranslator {
    fn name(&self) -> String {
        String::from("HierarchicalTranslator")
    }

    fn translates(&self, _name: &str) -> Result<bool> {
        Ok(true)
    }

    fn translate(&self, _signal: &Signal, value: &SignalValue) -> Result<TranslationResult> {
        Ok(TranslationResult {
            val: ValueRepr::Struct,
            subfields: vec![
                (
                    "a".to_string(),
                    TranslationResult {
                        val: ValueRepr::Bits(
                            10,
                            match value {
                                SignalValue::BigUint(v) => format!("{v:b}"),
                                SignalValue::String(v) => v.clone(),
                            },
                        ),
                        subfields: vec![],
                        durations: HashMap::new(),
                    },
                ),
                (
                    "b".to_string(),
                    TranslationResult {
                        val: ValueRepr::Bits(3, "11x".to_string()),
                        subfields: vec![],
                        durations: HashMap::new(),
                    },
                ),
                (
                    "c".to_string(),
                    TranslationResult {
                        val: ValueRepr::Tuple,
                        subfields: vec![
                            (
                                "0".to_string(),
                                TranslationResult {
                                    val: ValueRepr::Bits(3, "001".to_string()),
                                    subfields: vec![],
                                    durations: HashMap::new(),
                                },
                            ),
                            (
                                "1".to_string(),
                                TranslationResult {
                                    val: ValueRepr::Bits(4, "1111".to_string()),
                                    subfields: vec![],
                                    durations: HashMap::new(),
                                },
                            ),
                        ],
                        durations: HashMap::new(),
                    },
                ),
            ],
            durations: HashMap::new(),
        })
    }

    fn signal_info(&self, _signal: &Signal, _name: &str) -> Result<SignalInfo> {
        Ok(SignalInfo::Compound {
            subfields: vec![
                ("a".to_string(), SignalInfo::Bits),
                ("b".to_string(), SignalInfo::Bool),
                (
                    "c".to_string(),
                    SignalInfo::Compound {
                        subfields: vec![
                            ("0".to_string(), SignalInfo::Bits),
                            ("1".to_string(), SignalInfo::Bits),
                        ],
                    },
                ),
            ],
        })
    }
}
