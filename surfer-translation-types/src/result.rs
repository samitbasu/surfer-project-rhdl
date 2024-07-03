use crate::ValueKind;
use extism_convert::{FromBytes, Json, ToBytes};
use serde::{Deserialize, Serialize};

#[derive(Clone, FromBytes, ToBytes, Deserialize, Serialize)]
#[encoding(Json)]
pub struct TranslationResult {
    pub val: ValueRepr,
    pub subfields: Vec<SubFieldTranslationResult>,
    pub kind: ValueKind,
}

/// The representation of the value, compound values can be
/// be represented by the repr of their subfields
#[derive(Clone, Deserialize, Serialize)]
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
    /// The variable value is not present. This is used to draw variables which are
    /// validated by other variables.
    NotPresent,
}

#[derive(Clone)]
pub struct SubFieldFlatTranslationResult {
    pub names: Vec<String>,
    pub value: Option<TranslatedValue>,
}

// A tree of format results for a variable, to be flattened into `SubFieldFlatTranslationResult`s
pub struct HierFormatResult {
    pub names: Vec<String>,
    pub this: Option<TranslatedValue>,
    /// A list of subfields of arbitrary depth, flattened to remove hierarchy.
    /// i.e. `{a: {b: 0}, c: 0}` is flattened to `vec![a: {b: 0}, [a, b]: 0, c: 0]`
    pub fields: Vec<HierFormatResult>,
}

impl HierFormatResult {
    pub fn collect_into(self, into: &mut Vec<SubFieldFlatTranslationResult>) {
        into.push(SubFieldFlatTranslationResult {
            names: self.names,
            value: self.this,
        });
        self.fields.into_iter().for_each(|r| r.collect_into(into));
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SubFieldTranslationResult {
    pub name: String,
    pub result: TranslationResult,
}

impl SubFieldTranslationResult {
    pub fn new(name: impl ToString, result: TranslationResult) -> Self {
        SubFieldTranslationResult {
            name: name.to_string(),
            result,
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct TranslatedValue {
    pub value: String,
    pub kind: ValueKind,
}

impl TranslatedValue {
    pub fn from_basic_translate(result: (String, ValueKind)) -> Self {
        TranslatedValue {
            value: result.0,
            kind: result.1,
        }
    }

    pub fn new(value: impl ToString, kind: ValueKind) -> Self {
        TranslatedValue {
            value: value.to_string(),
            kind,
        }
    }
}
