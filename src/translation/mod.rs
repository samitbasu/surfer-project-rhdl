use std::collections::HashMap;

use color_eyre::Result;
use eframe::epaint::Color32;
use fastwave_backend::{Signal, SignalValue};

mod basic_translators;
pub mod pytranslator;
pub mod spade;

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

#[derive(Clone, PartialEq, Copy)]
pub enum ValueColor {
    Normal,
    Undef,
    HighImp,
    Custom(Color32),
    Warn,
}
impl ValueColor {
    pub(crate) fn to_color32(&self) -> Color32 {
        match self {
            ValueColor::Normal => Color32::GREEN,
            ValueColor::Undef => Color32::RED,
            ValueColor::HighImp => Color32::YELLOW,
            ValueColor::Warn => Color32::from_rgb(255, 102, 0),
            ValueColor::Custom(inner) => inner.clone(),
        }
    }
}

/// The representation of the value, compound values can be
/// be represented by the repr of their subfields
#[derive(Clone)]
pub enum ValueRepr {
    Bit(char),
    /// The value is `.0` raw bits, and can be translated by further translators
    Bits(u64, String),
    /// The value is exactly the specified string
    String(String),
    /// Represent the value as (f1, f2, f3...)
    Tuple,
    /// Represent the value as {f1: v1, f2: v2, f3: v3...}
    Struct,
    /// Represent as a spade-like enum with the specified field being shown.
    /// The index is the index of the option which is currently selected, the name is
    /// the name of that option to avoid having to look that up
    Enum {
        idx: usize,
        name: String,
    },
    /// Represent the value as [f1, f2, f3...]
    Array,
    /// The signal value is not present. This is used to draw signals which are
    /// validated by other signals.
    NotPresent,
}

pub struct FlatTranslationResult {
    /// The string representation of the translated result
    pub this: Option<(String, ValueColor)>,
    /// A list of subfields of arbitrary depth, flattened to remove hierarchy.
    /// i.e. `{a: {b: 0}, c: 0}` is flattened to `vec![a: {b: 0}, [a, b]: 0, c: 0]`
    pub fields: Vec<(Vec<String>, Option<(String, ValueColor)>)>,
}

impl FlatTranslationResult {
    pub fn as_fields(self) -> Vec<(Vec<String>, Option<(String, ValueColor)>)> {
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
    pub color: ValueColor,
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
            ValueRepr::Bit(val) => Some((format!("{val}"), self.color)),
            ValueRepr::Bits(bit_count, bits) => {
                let subtranslator_name = formats.get(&path_so_far).unwrap_or(&translators.default);

                let subtranslator = translators.basic.get(subtranslator_name).expect(&format!(
                    "Did not find a translator named {subtranslator_name}"
                ));

                let result = subtranslator
                    .as_ref()
                    .basic_translate(*bit_count, &SignalValue::String(bits.clone()));

                Some(result)
            }
            ValueRepr::String(sval) => Some((sval.clone(), self.color)),
            ValueRepr::Tuple => Some((
                format!(
                    "({})",
                    subresults
                        .iter()
                        .map(|(_, v)| v.this.as_ref().map(|t| t.0.as_str()).unwrap_or_else(|| "-"))
                        .join(", ")
                ),
                self.color,
            )),
            ValueRepr::Struct => Some((
                format!(
                    "{{{}}}",
                    subresults
                        .iter()
                        .map(|(n, v)| format!(
                            "{n}: {}",
                            v.this.as_ref().map(|t| t.0.as_str()).unwrap_or_else(|| "-")
                        ))
                        .join(", ")
                ),
                self.color,
            )),
            ValueRepr::Array => Some((
                format!(
                    "[{}]",
                    subresults
                        .iter()
                        .map(|(_, v)| v.this.as_ref().map(|t| t.0.as_str()).unwrap_or_else(|| "-"))
                        .join(", ")
                ),
                self.color,
            )),
            ValueRepr::NotPresent => None,
            ValueRepr::Enum { idx, name } => Some((
                format!(
                    "{name}{{{}}}",
                    subresults[*idx]
                        .1
                        .this
                        .as_ref()
                        .map(|t| t.0.as_str())
                        .unwrap_or_else(|| "-")
                ),
                self.color,
            )),
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

    fn translate(&self, signal: &Signal, value: &SignalValue) -> Result<TranslationResult>;

    fn signal_info(&self, signal: &Signal, _name: &str) -> Result<SignalInfo>;

    fn translates(&self, signal: &Signal) -> Result<bool>;
}

pub trait BasicTranslator {
    fn name(&self) -> String;
    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueColor);
}

impl Translator for Box<dyn BasicTranslator> {
    fn name(&self) -> String {
        self.as_ref().name()
    }

    fn translate(&self, signal: &Signal, value: &SignalValue) -> Result<TranslationResult> {
        let (val, color) = self
            .as_ref()
            .basic_translate(signal.num_bits().unwrap_or(0) as u64, value);
        Ok(TranslationResult {
            val: ValueRepr::String(val),
            color,
            subfields: vec![],
            durations: HashMap::new(),
        })
    }

    fn signal_info(&self, signal: &Signal, _name: &str) -> Result<SignalInfo> {
        if signal.num_bits() == Some(1) {
            Ok(SignalInfo::Bool)
        } else {
            Ok(SignalInfo::Bits)
        }
    }

    fn translates(&self, _signal: &Signal) -> Result<bool> {
        Ok(true)
    }
}
