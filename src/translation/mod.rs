use std::collections::HashMap;

use color_eyre::Result;
use eframe::epaint::Color32;
use fastwave_backend::{Signal, SignalType, SignalValue};

mod basic_translators;
pub mod clock;
pub mod numeric_translators;
pub mod spade;

pub use basic_translators::*;
use itertools::Itertools;
use num::BigUint;

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
            default: "Hexadecimal".to_string(),
            basic: basic.into_iter().map(|t| (t.name(), t)).collect(),
            inner: translators.into_iter().map(|t| (t.name(), t)).collect(),
        }
    }

    pub fn all_translator_names<'a>(&'a self) -> Vec<&'a String> {
        self.inner.keys().chain(self.basic.keys()).collect()
    }

    pub fn all_translators<'a>(&'a self) -> Vec<&'a dyn Translator> {
        // This is kind of inefficient, but I don't feel like messing with lifetimes
        // and downcasting BasicTranslator to Translator again. Since this function
        // isn't run very often, this should be sufficient
        self.all_translator_names()
            .into_iter()
            .map(|name| self.get_translator(name))
            .collect()
    }

    pub fn basic_translator_names<'a>(&'a self) -> Vec<&'a String> {
        self.basic.keys().collect()
    }

    pub fn get_translator<'a, 'b>(&'a self, name: &'b str) -> &'a dyn Translator {
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
pub enum ValueKind {
    Normal,
    Undef,
    HighImp,
    Custom(Color32),
    Warn,
    DontCare,
    Weak,
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
    pub this: Option<(String, ValueKind)>,
    /// A list of subfields of arbitrary depth, flattened to remove hierarchy.
    /// i.e. `{a: {b: 0}, c: 0}` is flattened to `vec![a: {b: 0}, [a, b]: 0, c: 0]`
    pub fields: Vec<(Vec<String>, Option<(String, ValueKind)>)>,
}

impl FlatTranslationResult {
    pub fn as_fields(self) -> Vec<(Vec<String>, Option<(String, ValueKind)>)> {
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
    pub color: ValueKind,
    /// Durations of different steps that were performed by the translator.
    /// Used for benchmarks
    pub durations: HashMap<String, f64>,
}

impl TranslationResult {
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
            ValueRepr::Bit(val) => {
                let subtranslator_name = formats.get(&path_so_far).unwrap_or(&translators.default);

                let subtranslator = translators.basic.get(subtranslator_name).expect(&format!(
                    "Did not find a translator named {subtranslator_name}"
                ));

                let result = subtranslator
                    .as_ref()
                    .basic_translate(1, &SignalValue::String(val.to_string()));

                Some(result)
            }
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
#[derive(Clone, Debug)]
pub enum SignalInfo {
    Compound {
        subfields: Vec<(String, SignalInfo)>,
    },
    Bits,
    Bool,
    Clock,
    String,
    Real,
}

impl SignalInfo {
    pub fn get_subinfo(&self, path: &[String]) -> &SignalInfo {
        match path {
            [] => self,
            [field, rest @ ..] => match self {
                SignalInfo::Compound { subfields } => subfields
                    .iter()
                    .find(|(f, _)| f == field)
                    .unwrap()
                    .1
                    .get_subinfo(rest),
                SignalInfo::Bits => panic!(),
                SignalInfo::Bool => panic!(),
                SignalInfo::Clock => panic!(),
                SignalInfo::String => panic!(),
                SignalInfo::Real => panic!(),
            },
        }
    }
}

pub enum TranslationPreference {
    /// This translator prefers translating the signal, so it will be selected
    /// as the default translator for the signal
    Prefer,
    /// This translator is able to translate the signal, but will not be
    /// selected by default, the user has to select it
    Yes,
    No,
}

pub fn translates_all_bit_types(signal: &Signal<'_>) -> Result<TranslationPreference> {
    if signal.signal_type() == Some(&SignalType::Str)
        || signal.signal_type() == Some(&SignalType::Real)
        || signal.signal_type() == Some(&SignalType::RealTime)
    {
        Ok(TranslationPreference::No)
    } else {
        Ok(TranslationPreference::Yes)
    }
}

pub trait Translator {
    fn name(&self) -> String;

    fn translate(&self, signal: &Signal, value: &SignalValue) -> Result<TranslationResult>;

    fn signal_info(&self, signal: &Signal, _name: &str) -> Result<SignalInfo>;

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference>;
}

pub trait BasicTranslator {
    fn name(&self) -> String;
    fn translate_biguint(&self, _: u64, _: BigUint) -> String {
        unimplemented!("Using default impl of translate_biguint")
    }
    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => (
                self.translate_biguint(num_bits, v.clone()),
                ValueKind::Normal,
            ),
            SignalValue::String(s) => match map_vector_signal(s) {
                NumberParseResult::Unparsable(v, k) => (v, k),
                NumberParseResult::Numerical(v) => {
                    (self.translate_biguint(num_bits, v), ValueKind::Normal)
                }
            },
        }
    }
    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        translates_all_bit_types(signal)
    }
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

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        self.as_ref().translates(signal)
    }
}

pub struct StringTranslator {}

impl Translator for StringTranslator {
    fn name(&self) -> String {
        "String".to_string()
    }

    fn translate(&self, _signal: &Signal, value: &SignalValue) -> Result<TranslationResult> {
        match value {
            SignalValue::BigUint(_) => panic!(),
            SignalValue::String(s) => Ok(TranslationResult {
                val: ValueRepr::String((*s).to_string()),
                color: ValueKind::Normal,
                subfields: vec![],
                durations: HashMap::new(),
            }),
        }
    }

    fn signal_info(&self, _signal: &Signal, _name: &str) -> Result<SignalInfo> {
        Ok(SignalInfo::String)
    }

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.signal_type() == Some(&SignalType::Str)
            || signal.signal_type() == Some(&SignalType::Real)
            || signal.signal_type() == Some(&SignalType::RealTime)
        {
            Ok(TranslationPreference::Prefer)
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

enum NumberParseResult {
    Numerical(BigUint),
    Unparsable(String, ValueKind),
}

/// Turn vector signal string into name and corresponding color if it
/// includes values other than 0 and 1. If only 0 and 1, return None.
fn map_vector_signal(s: &str) -> NumberParseResult {
    if let Some(val) = BigUint::parse_bytes(s.as_bytes(), 2) {
        NumberParseResult::Numerical(val)
    } else if s.contains('x') {
        NumberParseResult::Unparsable("UNDEF".to_string(), ValueKind::Undef)
    } else if s.contains('z') {
        NumberParseResult::Unparsable("HIGHIMP".to_string(), ValueKind::HighImp)
    } else if s.contains('-') {
        NumberParseResult::Unparsable("DON'T CARE".to_string(), ValueKind::DontCare)
    } else if s.contains('u') {
        NumberParseResult::Unparsable("UNDEF".to_string(), ValueKind::Undef)
    } else if s.contains('w') {
        NumberParseResult::Unparsable("UNDEF WEAK".to_string(), ValueKind::Undef)
    } else if s.contains('h') || s.contains('l') {
        NumberParseResult::Unparsable("WEAK".to_string(), ValueKind::Weak)
    } else {
        NumberParseResult::Unparsable("UNKNOWN VALUES".to_string(), ValueKind::Undef)
    }
}
