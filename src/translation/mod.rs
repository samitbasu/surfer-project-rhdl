use std::collections::HashMap;

use color_eyre::Result;
use fastwave_backend::{Signal, SignalValue};

mod basic_translators;
pub mod pytranslator;

pub use basic_translators::*;
use itertools::Itertools;

use crate::view::TraceIdx;

pub struct TranslatorList {
    inner: HashMap<String, Box<dyn Translator>>,
    basic: HashMap<String, Box<dyn BasicTranslator>>,
    pub default: String,
}

impl TranslatorList {
    pub fn new(
        basic: Vec<Box<dyn BasicTranslator>>,
        translators: Vec<Box<dyn Translator>>,
    ) -> Self {
        Self {
            default: basic.first().unwrap().name(),
            basic: basic.into_iter().map(|t| (t.name(), t)).collect(),
            inner: translators.into_iter().map(|t| (t.name(), t)).collect(),
        }
    }

    pub fn all_translator_names<'a>(&'a self) -> Vec<&'a String> {
        self.inner.keys().chain(self.basic.keys()).collect()
    }

    pub fn basic_translator_names<'a>(&'a self) -> Vec<&'a String> {
        self.basic.keys().collect()
    }

    pub fn get_translator<'a>(&'a self, name: &'a str) -> &'a dyn Translator {
        let full = self.inner.get(name);
        if let Some(full) = full.map(|t| t.as_ref()) {
            full
        } else {
            let basic = self.basic.get(name).unwrap();
            basic
        }
    }

    pub fn add(&mut self, t: Box<dyn Translator>) {
        self.inner.insert(t.name(), t);
    }
}

/// The representation of the value, compound values can be
/// be represented by the repr of their subfields
#[derive(Clone)]
pub enum ValueRepr {
    /// The value is `.0` raw bits, and can be translated by further translators
    Bits(u64, String),
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
    pub fn flatten(
        &self,
        path_so_far: TraceIdx,
        formats: &HashMap<TraceIdx, String>,
        translators: &TranslatorList,
    ) -> FlatTranslationResult {
        let subresults = self
            .subfields
            .iter()
            .map(|(n, v)| {
                let sub_path = path_so_far
                    .1
                    .clone()
                    .into_iter()
                    .chain(vec![n.clone()])
                    .collect();

                let sub = v.flatten((path_so_far.0, sub_path), formats, translators);
                (n, sub)
            })
            .collect::<Vec<_>>();

        let string_repr = match &self.val {
            ValueRepr::Bits(bit_count, bits) => {
                let subtranslator_name = formats.get(&path_so_far).unwrap_or(&translators.default);

                let subtranslator = translators.basic.get(subtranslator_name).expect(&format!(
                    "Did not find a translator named {subtranslator_name}"
                ));


                let result = subtranslator
                    .as_ref()
                    .basic_translate(*bit_count, &SignalValue::String(bits.clone()));

                result
            }
            ValueRepr::String(sval) => sval.clone(),
            ValueRepr::Tuple => {
                format!("({})", subresults.iter().map(|(_, v)| &v.this).join(", "))
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
                format!("[{}]", subresults.iter().map(|(_, v)| &v.this).join(", "))
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
    Bool,
}

pub trait Translator {
    fn name(&self) -> String;

    /// Return true if this translator translates the specified signal
    fn translates(&self, name: &str) -> Result<bool>;

    fn translate(&self, signal: &Signal, value: &SignalValue) -> Result<TranslationResult>;

    fn signal_info(&self, signal: &Signal, _name: &str) -> Result<SignalInfo>;
}

pub trait BasicTranslator {
    fn name(&self) -> String;
    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> String;
}

impl Translator for Box<dyn BasicTranslator> {
    fn name(&self) -> String {
        self.as_ref().name()
    }

    fn translates(&self, _name: &str) -> Result<bool> {
        Ok(true)
    }

    fn translate(&self, signal: &Signal, value: &SignalValue) -> Result<TranslationResult> {
        Ok(TranslationResult {
            val: ValueRepr::String(
                self.as_ref()
                    .basic_translate(signal.num_bits().unwrap_or(0) as u64, value),
            ),
            subfields: vec![],
            durations: HashMap::new(),
        })
    }

    fn signal_info(&self, signal: &Signal, _name: &str) -> Result<SignalInfo> {
        if signal.num_bits() == Some(0) {
            Ok(SignalInfo::Bool)
        } else {
            Ok(SignalInfo::Bits)
        }
    }
}
