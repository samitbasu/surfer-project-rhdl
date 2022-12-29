use std::collections::HashMap;

use color_eyre::Result;
use camino::Utf8PathBuf;
use fastwave_backend::{Signal, SignalValue};

pub mod pytranslator;

pub struct TranslatorList {
    pub inner: HashMap<String, Box<dyn Translator>>,
    pub default: String,
}

impl TranslatorList {
    pub fn new(translators: Vec<Box<dyn Translator>>) -> Self {
        Self {
            default: translators.first().unwrap().name(),
            inner: translators.into_iter().map(|t| (t.name(), t)).collect(),
        }
    }

    pub fn names(&self) -> Vec<String> {
        self.inner.keys().cloned().collect()
    }
}

pub struct SignalPath<'a> {
    hierarchy: &'a [String],
    name: &'a str,
}

pub struct TranslationResult {
    pub val: String,
    pub subfields: Vec<(String, Box<TranslationResult>)>,
}

pub trait Translator {
    fn name(&self) -> String;

    /// Return true if this translator translates the specified signal
    fn translates(&self, path: SignalPath) -> Result<bool>;

    fn translate(
        &self,
        signal: &Signal,
        value: &SignalValue,
    ) -> Result<TranslationResult>;
}

// Implementations

pub struct HexTranslator {}

impl Translator for HexTranslator {
    fn name(&self) -> String {
        String::from("Hexadecimal")
    }

    fn translates(&self, _path: SignalPath) -> Result<bool> {
        Ok(true)
    }

    fn translate(
        &self,
        signal: &Signal,
        value: &SignalValue,
    ) -> Result<TranslationResult> {
        let result = match value {
            SignalValue::BigUint(v) => TranslationResult {
                val: format!("{v:0width$x}", width=signal.num_bits().unwrap_or(0) as usize / 4),
                subfields: vec![],
            },
            SignalValue::String(s) => {
                // TODO: Translate hex values
                TranslationResult {
                    val: s.clone(),
                    subfields: vec![],
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

    fn translates(&self, _path: SignalPath) -> Result<bool> {
        Ok(true)
    }

    fn translate(
        &self,
        signal: &Signal,
        value: &SignalValue,
    ) -> Result<TranslationResult> {
        let result = match value {
            SignalValue::BigUint(v) => TranslationResult {
                val: format!("{v}"),
                subfields: vec![],
            },
            SignalValue::String(s) => {
                // TODO: Translate hex values
                TranslationResult {
                    val: s.clone(),
                    subfields: vec![],
                }
            }
        };
        Ok(result)
    }
}
