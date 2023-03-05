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

pub struct FlatTranslationResult {
    /// The string representation of the translated result
    pub this: String,
    /// A list of subfields of arbitrary depth, flattened to remove hierarchy.
    /// i.e. `{a: {b: 0}, c: 0}` is flattened to `vec![a: {b: 0}, [a, b]: 0, c: 0]`
    pub fields: Vec<(Vec<String>, String)>,
}

impl FlatTranslationResult {
    pub fn as_fields(self) -> Vec<(Vec<String>, String)> {
        vec![(vec![], self.this)]
            .into_iter()
            .chain(self.fields.into_iter())
            .collect()
    }
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

    /// Flattens the translation result into path, value pairs
    pub fn flatten(&self) -> FlatTranslationResult {
        let subresults = self
            .subfields
            .iter()
            .map(|(n, v)| {
                let sub = v.flatten();
                (n, sub)
            })
            .collect::<HashMap<_, _>>();

        let string_repr = match self.val {
            ValueRepr::Bits => "BITS PLACEHOLDER".to_string(),
            ValueRepr::String(sval) => sval.clone(),
            ValueRepr::Tuple => {
                format!("({})", subresults.iter().map(|(n, v)| v.this).join(", "))
            }
            ValueRepr::Struct => {
                format!(
                    "{{{}}}",
                    subresults
                        .iter()
                        .map(|(n, v)| format!("{n}: {}", v.this))
                        .join(", ")
                )
            }
            ValueRepr::Array => {
                format!("[{}]", subresults.iter().map(|(n, v)| v.this).join(", "))
            }
        };

        FlatTranslationResult {
            this: string_repr,
            fields: subresults
                .into_iter()
                .flat_map(|(n, sub)| {
                    sub.as_fields()
                        .into_iter()
                        .map(|(mut path, val)| {
                            path.insert(0, n.clone());
                            (path, val)
                        })
                        .collect::<Vec<_>>()
                })
                .collect(),
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
