use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use color_eyre::Result;
#[cfg(not(target_arch = "wasm32"))]
use directories::ProjectDirs;
use eframe::epaint::Color32;
#[cfg(not(target_arch = "wasm32"))]
use log::warn;
#[cfg(not(target_arch = "wasm32"))]
use toml::Table;

mod basic_translators;
pub mod clock;
mod enum_translator;
pub mod numeric_translators;
pub mod spade;

pub use basic_translators::*;
use instruction_decoder::Decoder;
use itertools::Itertools;
pub use numeric_translators::*;
use surfer_translation_types::{
    TranslationPreference, ValueKind, VariableEncoding, VariableInfo, VariableValue,
};

use crate::config::SurferTheme;
use crate::wave_container::{ScopeId, VarId};
use crate::{message::Message, wave_container::VariableMeta};

pub type DynBasicTranslator = dyn surfer_translation_types::BasicTranslator<VarId, ScopeId>;

/// Look inside the config directory and inside "$(cwd)/.surfer" for user-defined decoders
/// To add a new decoder named 'x', add a directory 'x' to the decoders directory
/// Inside, multiple toml files can be added which will all be used for decoding 'x'
/// This is useful e.g., for layering RISC-V extensions
#[cfg(not(target_arch = "wasm32"))]
fn find_user_decoders() -> Vec<Box<DynBasicTranslator>> {
    let mut decoders: Vec<Box<DynBasicTranslator>> = vec![];
    if let Some(proj_dirs) = ProjectDirs::from("org", "surfer-project", "surfer") {
        let mut config_decoders = find_user_decoders_at_path(proj_dirs.config_dir());
        decoders.append(&mut config_decoders);
    }

    let mut project_decoders = find_user_decoders_at_path(Path::new(".surfer"));
    decoders.append(&mut project_decoders);

    decoders
}

/// Look for user defined decoders in path.
#[cfg(not(target_arch = "wasm32"))]
fn find_user_decoders_at_path(path: &Path) -> Vec<Box<DynBasicTranslator>> {
    let mut decoders: Vec<Box<DynBasicTranslator>> = vec![];
    let Ok(decoder_dirs) = std::fs::read_dir(path.join("decoders")) else {
        return decoders;
    };

    for decoder_dir in decoder_dirs.flatten() {
        if decoder_dir.metadata().is_ok_and(|m| m.is_dir()) {
            let Ok(name) = decoder_dir.file_name().into_string() else {
                warn!("Cannot load decoder. Invalid name.");
                continue;
            };
            let mut tomls = vec![];
            // Keeps track of the bit width of the first parsed toml
            // All tomls must use the same width
            let mut width: Option<toml::Value> = None;

            if let Ok(toml_files) = std::fs::read_dir(decoder_dir.path()) {
                for toml_file in toml_files.flatten() {
                    if toml_file
                        .file_name()
                        .into_string()
                        .is_ok_and(|file_name| file_name.ends_with(".toml"))
                    {
                        let Ok(text) = std::fs::read_to_string(toml_file.path()) else {
                            warn!(
                                "Skipping toml file {:?}. Cannot read file.",
                                toml_file.path()
                            );
                            continue;
                        };

                        let Ok(toml_parsed) = text.parse::<Table>() else {
                            warn!(
                                "Skipping toml file {:?}. Cannot parse toml.",
                                toml_file.path()
                            );
                            continue;
                        };

                        let Some(toml_width) = toml_parsed.get("width") else {
                            warn!(
                                "Skipping toml file {:?}. Mandatory key 'width' is missing.",
                                toml_file.path()
                            );
                            continue;
                        };

                        if width.clone().is_some_and(|width| width != *toml_width) {
                            warn!(
                                "Skipping toml file {:?}. Bit widths do not match.",
                                toml_file.path()
                            );
                            continue;
                        } else {
                            width = Some(toml_width.clone());
                        }

                        tomls.push(toml_parsed)
                    }
                }
            }

            if let Some(width) = width.and_then(|width| width.as_integer()) {
                decoders.push(Box::new(InstructionTranslator {
                    name,
                    decoder: Decoder::new_from_table(tomls),
                    num_bits: width.unsigned_abs(),
                }));
            }
        }
    }
    decoders
}

pub fn all_translators() -> TranslatorList {
    let mut basic_decoders: Vec<Box<DynBasicTranslator>> = vec![
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
            name: "RV32".into(),
            decoder: Decoder::new(&[
                include_str!("../../../instruction-decoder/toml/RV32I.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV32M.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV32Zicsr.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV32F.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV32A.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV32D.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV32V.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV32_Zbb.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV32_Zbs.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV_Zba.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV_Zbc.toml").to_string(),
            ]),
            num_bits: 32,
        }),
        Box::new(InstructionTranslator {
            name: "RV64".into(),
            decoder: Decoder::new(&[
                include_str!("../../../instruction-decoder/toml/RV64_Zbb.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV64_Zbs.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV_Zba.toml").to_string(),
                include_str!("../../../instruction-decoder/toml/RV_Zbc.toml").to_string(),
            ]),
            num_bits: 32,
        }),
        Box::new(InstructionTranslator {
            name: "Mips".into(),
            decoder: Decoder::new(&[
                include_str!("../../../instruction-decoder/toml/mips.toml").to_string()
            ]),
            num_bits: 32,
        }),
        Box::new(LebTranslator {}),
        #[cfg(feature = "f128")]
        Box::new(QuadPrecisionTranslator {}),
    ];

    #[cfg(not(target_arch = "wasm32"))]
    basic_decoders.append(&mut find_user_decoders());

    TranslatorList::new(
        basic_decoders,
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
    basic: HashMap<String, Box<DynBasicTranslator>>,
    pub default: String,
}

impl TranslatorList {
    pub fn new(basic: Vec<Box<DynBasicTranslator>>, translators: Vec<Box<dyn Translator>>) -> Self {
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

#[local_impl::local_impl]
impl VariableInfoExt for VariableInfo {
    fn get_subinfo(&self, path: &[String]) -> &VariableInfo {
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

    fn has_subpath(&self, path: &[String]) -> bool {
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

#[local_impl::local_impl]
impl ValueKindExt for ValueKind {
    fn color(&self, user_color: Color32, theme: &SurferTheme) -> Color32 {
        match self {
            ValueKind::HighImp => theme.variable_highimp,
            ValueKind::Undef => theme.variable_undef,
            ValueKind::DontCare => theme.variable_dontcare,
            ValueKind::Warn => theme.variable_undef,
            ValueKind::Custom([r, g, b, a]) => Color32::from_rgba_unmultiplied(*r, *g, *b, *a),
            ValueKind::Weak => theme.variable_weak,
            ValueKind::Normal => user_color,
        }
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

impl Translator for Box<DynBasicTranslator> {
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
        // f64 (i.e. "real") values are treated as strings for now
        if variable.encoding == VariableEncoding::String
            || variable.encoding == VariableEncoding::Real
        {
            Ok(TranslationPreference::Prefer)
        } else {
            Ok(TranslationPreference::No)
        }
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
