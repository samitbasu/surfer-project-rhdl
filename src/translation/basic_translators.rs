use std::collections::HashMap;

use super::{SignalInfo, TranslationResult, Translator, ValueRepr};

use color_eyre::Result;
use fastwave_backend::{Signal, SignalValue};

pub struct HexTranslator {}

impl Translator for HexTranslator {
    fn name(&self) -> String {
        String::from("Hexadecimal")
    }

    fn translates(&self, _name: &str) -> Result<bool> {
        Ok(true)
    }

    fn translate(&self, signal: &Signal, value: &SignalValue) -> Result<TranslationResult> {
        let result = match value {
            SignalValue::BigUint(v) => TranslationResult {
                val: ValueRepr::String(format!(
                    "{v:0width$x}",
                    width = signal.num_bits().unwrap_or(0) as usize / 4
                )),
                subfields: vec![],
                durations: HashMap::new(),
            },
            SignalValue::String(s) => {
                // TODO: Translate hex values
                TranslationResult {
                    val: ValueRepr::String(s.clone()),
                    subfields: vec![],
                    durations: HashMap::new(),
                }
            }
        };
        Ok(result)
    }

    fn signal_info(&self, signal: &Signal, _name: &str) -> Result<SignalInfo> {
        if signal.num_bits() == Some(1) {
            Ok(SignalInfo::Bool)
        } else {
            Ok(SignalInfo::Bits)
        }
    }
}

pub struct UnsignedTranslator {}

impl Translator for UnsignedTranslator {
    fn name(&self) -> String {
        String::from("Unsigned")
    }

    fn translates(&self, _name: &str) -> Result<bool> {
        Ok(true)
    }

    fn translate(&self, _signal: &Signal, value: &SignalValue) -> Result<TranslationResult> {
        let result = match value {
            SignalValue::BigUint(v) => TranslationResult {
                val: ValueRepr::String(format!("{v}")),
                subfields: vec![],
                durations: HashMap::new(),
            },
            SignalValue::String(s) => {
                // TODO: Translate hex values
                TranslationResult {
                    val: ValueRepr::String(s.clone()),
                    subfields: vec![],
                    durations: HashMap::new(),
                }
            }
        };
        Ok(result)
    }

    fn signal_info(&self, signal: &Signal, _name: &str) -> Result<SignalInfo> {
        if signal.num_bits() == Some(1) {
            Ok(SignalInfo::Bool)
        } else {
            Ok(SignalInfo::Bits)
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
                ("a".to_string(), TranslationResult {
                    val: ValueRepr::Bits(match value {
                        SignalValue::BigUint(v) => format!("{v:b}"),
                        SignalValue::String(v) => v.clone(),
                    }),
                    subfields: vec![],
                    durations: HashMap::new()
                }),
                ("b".to_string(), TranslationResult {
                    val: ValueRepr::Bits("11x".to_string()),
                    subfields: vec![],
                    durations: HashMap::new()
                }),
                ("c".to_string(), TranslationResult {
                    val: ValueRepr::Tuple,
                    subfields: vec![
                        ("0".to_string(), TranslationResult {
                            val: ValueRepr::Bits("001".to_string()),
                            subfields: vec![],
                            durations: HashMap::new()
                        }),
                        ("1".to_string(), TranslationResult {
                            val: ValueRepr::Bits("1111".to_string()),
                            subfields: vec![],
                            durations: HashMap::new()
                        }),
                    ],
                    durations: HashMap::new()
                })
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
