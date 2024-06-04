use std::{collections::HashMap, sync::mpsc::Sender};

use color_eyre::Result;
use eframe::epaint::Color32;

mod basic_translators;
pub mod clock;
mod enum_translator;
pub mod numeric_translators;
pub mod spade;

pub use basic_translators::*;
use instruction_decoder::Decoder;
use itertools::Itertools;
use num::BigUint;
pub use numeric_translators::*;

use crate::{
    message::Message,
    variable_type::STRING_TYPES,
    wave_container::{VariableMeta, VariableValue},
};

pub fn all_translators() -> TranslatorList {
    TranslatorList::new(
        vec![
            Box::new(BitTranslator {}),
            Box::new(HexTranslator {}),
            Box::new(OctalTranslator {}),
            Box::new(UnsignedTranslator {}),
            Box::new(SignedTranslator {}),
            Box::new(GroupingBinaryTranslator {}),
            Box::new(BinaryTranslator {}),
            Box::new(ASCIITranslator {}),
            Box::new(SinglePrecisionTranslator {}),
            Box::new(DoublePrecisionTranslator {}),
            Box::new(HalfPrecisionTranslator {}),
            Box::new(BFloat16Translator {}),
            Box::new(Posit32Translator {}),
            Box::new(Posit16Translator {}),
            Box::new(Posit8Translator {}),
            Box::new(PositQuire8Translator {}),
            Box::new(PositQuire16Translator {}),
            Box::new(E5M2Translator {}),
            Box::new(E4M3Translator {}),
            Box::new(InstructionTranslator {
                name: "RV32IMAFDZicsr".into(),
                decoder: Decoder::new(&vec![
                    include_str!("../../instruction-decoder/toml/RV32I.toml").to_string(),
                    include_str!("../../instruction-decoder/toml/RV32M.toml").to_string(),
                    include_str!("../../instruction-decoder/toml/RV32A.toml").to_string(),
                    include_str!("../../instruction-decoder/toml/RV32F.toml").to_string(),
                    include_str!("../../instruction-decoder/toml/RV32D.toml").to_string(),
                ]),
            }),
            Box::new(InstructionTranslator {
                name: "Mips".into(),
                decoder: Decoder::new(&[
                    include_str!("../../instruction-decoder/toml/mips.toml").to_string()
                ]),
            }),
            Box::new(LebTranslator {}),
            #[cfg(feature = "f128")]
            Box::new(QuadPrecisionTranslator {}),
        ],
        vec![
            Box::new(clock::ClockTranslator::new()),
            Box::new(StringTranslator {}),
            Box::new(enum_translator::EnumTranslator {}),
        ],
    )
}

#[derive(Default)]
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

    pub fn all_translator_names(&self) -> Vec<&String> {
        self.inner.keys().chain(self.basic.keys()).collect()
    }

    pub fn all_translators(&self) -> Vec<&dyn Translator> {
        // This is kind of inefficient, but I don't feel like messing with lifetimes
        // and downcasting BasicTranslator to Translator again. Since this function
        // isn't run very often, this should be sufficient
        self.all_translator_names()
            .into_iter()
            .map(|name| self.get_translator(name))
            .collect()
    }

    pub fn basic_translator_names(&self) -> Vec<&String> {
        self.basic.keys().collect()
    }

    pub fn get_translator(&self, name: &str) -> &(dyn Translator) {
        let full = self.inner.get(name);
        if let Some(full) = full.map(|t| t.as_ref()) {
            full
        } else {
            let basic = self
                .basic
                .get(name)
                .unwrap_or_else(|| panic! {"No translator called {name}"});
            basic
        }
    }

    pub fn add_or_replace(&mut self, t: Box<dyn Translator>) {
        self.inner.insert(t.name(), t);
    }

    pub fn is_valid_translator(&self, meta: &VariableMeta, candidate: &str) -> bool {
        self.get_translator(candidate)
            .translates(meta)
            .map(|preference| preference != TranslationPreference::No)
            .unwrap_or(false)
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
    /// The variable value is not present. This is used to draw variables which are
    /// validated by other variables.
    NotPresent,
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

#[derive(Clone)]
pub struct SubFieldFlatTranslationResult {
    pub names: Vec<String>,
    pub value: Option<TranslatedValue>,
}

// A tree of format results for a variable, to be flattened into `SubFieldFlatTranslationResult`s
struct HierFormatResult {
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

#[derive(Clone)]
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

#[derive(Clone)]
pub struct TranslationResult {
    pub val: ValueRepr,
    pub subfields: Vec<SubFieldTranslationResult>,
    pub kind: ValueKind,
}

fn format(
    val: &ValueRepr,
    kind: ValueKind,
    subtranslator_name: &String,
    translators: &TranslatorList,
    subresults: &[HierFormatResult],
) -> Option<TranslatedValue> {
    match val {
        ValueRepr::Bit(val) => {
            let subtranslator = translators
                .basic
                .get(subtranslator_name)
                .unwrap_or_else(|| panic!("Did not find a translator named {subtranslator_name}"));

            Some(TranslatedValue::from_basic_translate(
                subtranslator
                    .as_ref()
                    .basic_translate(1, &VariableValue::String(val.to_string())),
            ))
        }
        ValueRepr::Bits(bit_count, bits) => {
            let subtranslator = translators
                .basic
                .get(subtranslator_name)
                .unwrap_or_else(|| panic!("Did not find a translator named {subtranslator_name}"));

            Some(TranslatedValue::from_basic_translate(
                subtranslator
                    .as_ref()
                    .basic_translate(*bit_count, &VariableValue::String(bits.clone())),
            ))
        }
        ValueRepr::String(sval) => Some(TranslatedValue {
            value: sval.clone(),
            kind,
        }),
        ValueRepr::Tuple => Some(TranslatedValue {
            value: format!(
                "({})",
                subresults
                    .iter()
                    .map(|v| v
                        .this
                        .as_ref()
                        .map(|t| t.value.as_str())
                        .unwrap_or_else(|| "-"))
                    .join(", ")
            ),
            kind,
        }),
        ValueRepr::Struct => Some(TranslatedValue {
            value: format!(
                "{{{}}}",
                subresults
                    .iter()
                    .map(|v| {
                        let n = v.names.join("_");
                        format!(
                            "{n}: {}",
                            v.this
                                .as_ref()
                                .map(|t| t.value.as_str())
                                .unwrap_or_else(|| "-")
                        )
                    })
                    .join(", ")
            ),
            kind,
        }),
        ValueRepr::Array => Some(TranslatedValue {
            value: format!(
                "[{}]",
                subresults
                    .iter()
                    .map(|v| v
                        .this
                        .as_ref()
                        .map(|t| t.value.as_str())
                        .unwrap_or_else(|| "-"))
                    .join(", ")
            ),
            kind,
        }),
        ValueRepr::NotPresent => None,
        ValueRepr::Enum { idx, name } => Some(TranslatedValue {
            value: format!(
                "{name}{{{}}}",
                subresults[*idx]
                    .this
                    .as_ref()
                    .map(|t| t.value.as_str())
                    .unwrap_or_else(|| "-")
            ),
            kind,
        }),
    }
}

impl TranslationResult {
    fn sub_format(
        &self,
        formats: &[crate::displayed_item::FieldFormat],
        translators: &TranslatorList,
        path_so_far: &[String],
    ) -> Vec<HierFormatResult> {
        self.subfields
            .iter()
            .map(|res| {
                let sub_path = path_so_far
                    .iter()
                    .chain([&res.name])
                    .cloned()
                    .collect::<Vec<_>>();

                let sub = res.result.sub_format(formats, translators, &sub_path);

                // we can consistently fall back to the default here since sub-fields
                // are never checked for their preferred translator
                let translator_name = formats
                    .iter()
                    .find(|e| e.field == sub_path)
                    .map(|e| e.format.clone())
                    .unwrap_or(translators.default.clone());
                let formatted = format(
                    &res.result.val,
                    res.result.kind,
                    &translator_name,
                    translators,
                    &sub,
                );

                HierFormatResult {
                    this: formatted,
                    names: sub_path,
                    fields: sub,
                }
            })
            .collect::<Vec<_>>()
    }

    /// Flattens the translation result into path, value pairs
    pub fn format_flat(
        &self,
        root_format: &Option<String>,
        formats: &[crate::displayed_item::FieldFormat],
        translators: &TranslatorList,
    ) -> Vec<SubFieldFlatTranslationResult> {
        let sub_result = self.sub_format(formats, translators, &[]);

        // FIXME for consistency we should not fall back to `translators.default` here, but fetch the
        // preferred translator - but doing that ATM will break if the spade translator is used, since
        // on the first render the spade translator seems to not have loaded it's information yet.
        let formatted = format(
            &self.val,
            self.kind,
            root_format.as_ref().unwrap_or(&translators.default),
            translators,
            &sub_result,
        );

        let formatted = HierFormatResult {
            names: vec![],
            this: formatted,
            fields: sub_result,
        };
        let mut collected = vec![];
        formatted.collect_into(&mut collected);
        collected
    }
}

/// Static information about the structure of a variable.
#[derive(Clone, Debug, Default)]
pub enum VariableInfo {
    Compound {
        subfields: Vec<(String, VariableInfo)>,
    },
    Bits,
    Bool,
    Clock,
    // NOTE: only used for state saving where translators will clear this out with the actual value
    #[default]
    String,
    Real,
}

impl VariableInfo {
    pub fn get_subinfo(&self, path: &[String]) -> &VariableInfo {
        match path {
            [] => self,
            [field, rest @ ..] => match self {
                VariableInfo::Compound { subfields } => subfields
                    .iter()
                    .find(|(f, _)| f == field)
                    .unwrap()
                    .1
                    .get_subinfo(rest),
                VariableInfo::Bits => panic!(),
                VariableInfo::Bool => panic!(),
                VariableInfo::Clock => panic!(),
                VariableInfo::String => panic!(),
                VariableInfo::Real => panic!(),
            },
        }
    }

    pub fn has_subpath(&self, path: &[String]) -> bool {
        match path {
            [] => true,
            [field, rest @ ..] => match self {
                VariableInfo::Compound { subfields } => subfields
                    .iter()
                    .find(|&(f, _)| f == field)
                    .map(|(_, info)| info.has_subpath(rest))
                    .unwrap_or(false),
                _ => false,
            },
        }
    }
}

#[derive(PartialEq)]
pub enum TranslationPreference {
    /// This translator prefers translating the variable, so it will be selected
    /// as the default translator for the variable
    Prefer,
    /// This translator is able to translate the variable, but will not be
    /// selected by default, the user has to select it
    Yes,
    No,
}

pub fn translates_all_bit_types(variable: &VariableMeta) -> Result<TranslationPreference> {
    if STRING_TYPES.contains(&variable.variable_type) {
        Ok(TranslationPreference::No)
    } else {
        Ok(TranslationPreference::Yes)
    }
}

pub trait Translator: Send + Sync {
    fn name(&self) -> String;

    fn translate(
        &self,
        variable: &VariableMeta,
        value: &VariableValue,
    ) -> Result<TranslationResult>;

    fn variable_info(&self, variable: &VariableMeta) -> Result<VariableInfo>;

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference>;

    // By default translators are stateless, but if they need to reload, they can
    // do by defining this method.
    // Long running translators should run the reloading in the background using `perform_work`
    fn reload(&self, _sender: Sender<Message>) {}
}

pub trait BasicTranslator: Send + Sync {
    fn name(&self) -> String;
    fn basic_translate(&self, num_bits: u64, value: &VariableValue) -> (String, ValueKind);
    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        translates_all_bit_types(variable)
    }
    fn variable_info(&self, _variable: &VariableMeta) -> Result<VariableInfo> {
        Ok(VariableInfo::Bits)
    }
}

impl Translator for Box<dyn BasicTranslator> {
    fn name(&self) -> String {
        self.as_ref().name()
    }

    fn translate(
        &self,
        variable: &VariableMeta,
        value: &VariableValue,
    ) -> Result<TranslationResult> {
        let (val, kind) = self
            .as_ref()
            .basic_translate(variable.num_bits.unwrap_or(0) as u64, value);
        Ok(TranslationResult {
            val: ValueRepr::String(val),
            kind,
            subfields: vec![],
        })
    }

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        self.as_ref().translates(variable)
    }

    fn variable_info(&self, variable: &VariableMeta) -> Result<VariableInfo> {
        self.as_ref().variable_info(variable)
    }
}

pub struct StringTranslator {}

impl Translator for StringTranslator {
    fn name(&self) -> String {
        "String".to_string()
    }

    fn translate(
        &self,
        _variable: &VariableMeta,
        value: &VariableValue,
    ) -> Result<TranslationResult> {
        match value {
            VariableValue::BigUint(b) => Ok(TranslationResult {
                val: ValueRepr::String(format!("ERROR (0x{:x})", b)),
                kind: ValueKind::Warn,
                subfields: vec![],
            }),
            VariableValue::String(s) => Ok(TranslationResult {
                val: ValueRepr::String((*s).to_string()),
                kind: ValueKind::Normal,
                subfields: vec![],
            }),
        }
    }

    fn variable_info(&self, _variable: &VariableMeta) -> Result<VariableInfo> {
        Ok(VariableInfo::String)
    }

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        if STRING_TYPES.contains(&variable.variable_type) {
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

/// Turn vector variable string into name and corresponding kind if it
/// includes values other than 0 and 1. If only 0 and 1, return None.
fn map_vector_variable(s: &str) -> NumberParseResult {
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

fn check_single_wordlength(num_bits: Option<u32>, required: u32) -> Result<TranslationPreference> {
    if let Some(num_bits) = num_bits {
        if num_bits == required {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    } else {
        Ok(TranslationPreference::No)
    }
}
