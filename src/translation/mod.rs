use std::collections::HashMap;

use color_eyre::Result;
use fastwave_backend::{Signal, SignalValue};

mod basic_translators;
pub mod pytranslator;

pub use basic_translators::*;
use itertools::Itertools;

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

/// The representation of the value, compound values can be
/// be represented by the repr of their subfields
#[derive(Clone)]
pub enum ValueRepr {
    /// The value is raw bits, and can be translated by further translators
    Bits,
    /// The value is exactly the specified string
    String(String),
    /// Represent the value as (f1, f2, f3...)
    Tuple,
    /// Represent the value as {f1: v1, f2: v2, f3: v3...}
    Struct,
    /// Represent the value as [f1, f2, f3...]
    Array,
}

#[derive(Clone)]
pub struct TranslationResult {
    pub val: ValueRepr,
    pub subfields: Vec<(String, TranslationResult)>,
    /// Durations of different steps that were performed by the translator.
    /// Used for benchmarks
    pub durations: HashMap<String, f64>,
}

impl TranslationResult {
    fn push_duration(&mut self, name: &str, val: f64) {
        self.durations.insert(name.to_string(), val);
    }

    pub fn to_string(&self) -> String {
        match &self.val {
            ValueRepr::Bits => format!("BITS PLACEHOLDER"),
            ValueRepr::String(raw) => raw.clone(),
            ValueRepr::Tuple => {
                format!(
                    "({})",
                    self.subfields.iter().map(|(_, f)| f.to_string()).join(", ")
                )
            }
            ValueRepr::Struct => {
                format!(
                    "{{{}}}",
                    self.subfields
                        .iter()
                        .map(|(n, f)| format!("{n}: {}", f.to_string()))
                        .join(", ")
                )
            }
            ValueRepr::Array => {
                format!(
                    "[{}]",
                    self.subfields.iter().map(|(_, f)| f.to_string()).join(", ")
                )
            }
        }
    }
}

/// Static information about the structure of a signal.
#[derive(Clone)]
pub enum SignalInfo {
    Compound {
        subfields: Vec<(String, SignalInfo)>,
    },
    Bits,
}

pub trait Translator {
    fn name(&self) -> String;

    /// Return true if this translator translates the specified signal
    fn translates(&self, name: &str) -> Result<bool>;

    fn translate(&self, signal: &Signal, value: &SignalValue) -> Result<TranslationResult>;

    fn signal_info(&self, _name: &str) -> Result<SignalInfo> {
        Ok(SignalInfo::Bits)
    }
}
