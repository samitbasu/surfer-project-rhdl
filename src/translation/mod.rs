use std::collections::HashMap;

use color_eyre::Result;
use fastwave_backend::{Signal, SignalValue};

pub mod pytranslator;
mod basic_translators;

pub use basic_translators::*;

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

    pub fn add(&mut self, t: Box<dyn Translator>) {
        self.inner.insert(t.name(), t);
    }
}

#[derive(Clone)]
pub struct TranslationResult {
    pub val: String,
    pub subfields: Vec<(String, TranslationResult)>,
    /// Durations of different steps that were performed by the translator.
    /// Used for benchmarks
    pub durations: HashMap<String, f64>
}


impl TranslationResult {
    fn push_duration(&mut self, name: &str, val: f64) {
        self.durations.insert(name.to_string(), val);
    }
}

/// Static information about the structure of a signal.
#[derive(Clone)]
pub enum SignalInfo {
    Compound{subfields: Vec<(String, SignalInfo)>},
    Bits
}

pub trait Translator {
    fn name(&self) -> String;

    /// Return true if this translator translates the specified signal
    fn translates(&self, name: &str) -> Result<bool>;

    fn translate(
        &self,
        signal: &Signal,
        value: &SignalValue,
    ) -> Result<TranslationResult>;

    fn signal_info(&self, _name: &str) -> Result<SignalInfo> {
        Ok(SignalInfo::Bits)
    }
}

