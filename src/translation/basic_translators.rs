use super::{SignalInfo, TranslationResult, Translator};

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
                val: format!(
                    "{v:0width$x}",
                    width = signal.num_bits().unwrap_or(0) as usize / 4
                ),
                subfields: vec![],
                duration: None
            },
            SignalValue::String(s) => {
                // TODO: Translate hex values
                TranslationResult {
                    val: s.clone(),
                    subfields: vec![],
                    duration: None
                }
            }
        };
        Ok(result)
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
                val: format!("{v}"),
                subfields: vec![],
                duration: None
            },
            SignalValue::String(s) => {
                // TODO: Translate hex values
                TranslationResult {
                    val: s.clone(),
                    subfields: vec![],
                    duration: None
                }
            }
        };
        Ok(result)
    }
}

pub struct HierarchyTranslator {}

impl Translator for HierarchyTranslator {
    fn name(&self) -> String {
        String::from("Hierarchy")
    }

    fn translates(&self, _name: &str) -> Result<bool> {
        Ok(true)
    }

    fn translate(&self, _signal: &Signal, value: &SignalValue) -> Result<TranslationResult> {
        Ok(TranslationResult {
            val: format!("hierarchy {value:?}"),
            subfields: vec![
                (
                    "f1".to_string(),
                    TranslationResult {
                        val: format!("field1"),
                        subfields: vec![],
                        duration: None
                    },
                ),
                (
                    "f2".to_string(),
                    TranslationResult {
                        val: format!("field1"),
                        subfields: vec![],
                        duration: None
                    },
                ),
            ],
            duration: None
        })
    }

    fn signal_info(&self, _name: &str) -> Result<SignalInfo> {
        Ok(SignalInfo::Compound {
            subfields: vec![
                ("f1".to_string(), SignalInfo::Bits),
                ("f2".to_string(), SignalInfo::Bits),
            ],
        })
    }
}
