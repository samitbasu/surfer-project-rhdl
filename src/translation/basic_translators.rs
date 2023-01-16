use std::collections::HashMap;

use super::{TranslationResult, Translator, ValueRepr};

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
}
