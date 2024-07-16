use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::sync::mpsc::Sender;

use color_eyre::Result;
#[cfg(not(target_arch = "wasm32"))]
use directories::ProjectDirs;
use ecolor::Color32;
#[cfg(not(target_arch = "wasm32"))]
use log::warn;
#[cfg(not(target_arch = "wasm32"))]
use toml::Table;

mod basic_translators;
pub mod clock;
mod enum_translator;
mod instruction_translators;
pub mod numeric_translators;
pub mod spade;

pub use basic_translators::*;
use clock::ClockTranslator;
use instruction_decoder::Decoder;
pub use instruction_translators::*;
use itertools::Itertools;
pub use numeric_translators::*;
use surfer_translation_types::{
    BasicTranslator, HierFormatResult, NumericTranslator, SubFieldFlatTranslationResult,
    TranslatedValue, TranslationPreference, TranslationResult, Translator, ValueKind, ValueRepr,
    VariableEncoding, VariableInfo, VariableValue,
};

use crate::config::SurferTheme;
use crate::wave_container::{ScopeId, VarId};
use crate::{message::Message, wave_container::VariableMeta};

pub type DynTranslator = dyn Translator<VarId, ScopeId, Message>;
pub type DynBasicTranslator = dyn BasicTranslator<VarId, ScopeId>;
pub type DynNumericTranslator = dyn NumericTranslator<VarId, ScopeId, Message>;

pub enum AnyTranslator {
    Full(Box<DynTranslator>),
    Basic(Box<DynBasicTranslator>),
    Numeric(Box<DynNumericTranslator>),
}

impl AnyTranslator {
    pub fn is_basic(&self) -> bool {
        matches!(self, AnyTranslator::Basic(_))
    }
}

impl Translator<VarId, ScopeId, Message> for AnyTranslator {
    fn name(&self) -> String {
        match self {
            AnyTranslator::Full(t) => t.name(),
            AnyTranslator::Basic(t) => t.name(),
            AnyTranslator::Numeric(t) => t.name(),
        }
    }

    fn translate(
        &self,
        variable: &VariableMeta,
        value: &VariableValue,
    ) -> Result<TranslationResult> {
        match self {
            AnyTranslator::Full(t) => t.translate(variable, value),
            AnyTranslator::Basic(t) => {
                let (val, kind) = t.basic_translate(variable.num_bits.unwrap_or(0) as u64, value);
                Ok(TranslationResult {
                    val: ValueRepr::String(val),
                    kind,
                    subfields: vec![],
                })
            }
            AnyTranslator::Numeric(t) => {
                let num_bits = variable.num_bits.unwrap_or(0) as u64;
                let (val, kind) = match value.clone().parse_biguint() {
                    Ok(v) => (t.translate_biguint(num_bits, v), ValueKind::Normal),
                    Err((v, k)) => (v, k),
                };
                Ok(TranslationResult {
                    val: ValueRepr::String(val),
                    kind,
                    subfields: vec![],
                })
            }
        }
    }

    fn variable_info(&self, variable: &VariableMeta) -> Result<VariableInfo> {
        match self {
            AnyTranslator::Full(t) => t.variable_info(variable),
            AnyTranslator::Basic(t) => t.variable_info(variable),
            AnyTranslator::Numeric(t) => t.variable_info(variable),
        }
    }

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        match self {
            AnyTranslator::Full(t) => t.translates(variable),
            AnyTranslator::Basic(t) => t.translates(variable),
            AnyTranslator::Numeric(t) => t.translates(variable),
        }
    }

    fn reload(&self, sender: Sender<Message>) {
        match self {
            AnyTranslator::Full(t) => t.reload(sender),
            AnyTranslator::Basic(_) => (),
            AnyTranslator::Numeric(t) => t.reload(sender),
        }
    }
}

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
    let mut basic_translators: Vec<Box<DynBasicTranslator>> = vec![
        Box::new(BitTranslator {}),
        Box::new(HexTranslator {}),
        Box::new(OctalTranslator {}),
        Box::new(GroupingBinaryTranslator {}),
        Box::new(BinaryTranslator {}),
        Box::new(ASCIITranslator {}),
        Box::new(new_rv32_translator()),
        Box::new(new_rv64_translator()),
        Box::new(new_mips_translator()),
        Box::new(LebTranslator {}),
        Box::new(NumberOfOnesTranslator {}),
    ];

    #[cfg(not(target_arch = "wasm32"))]
    basic_translators.append(&mut find_user_decoders());

    let numeric_translators: Vec<Box<DynNumericTranslator>> = vec![
        Box::new(UnsignedTranslator {}),
        Box::new(SignedTranslator {}),
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
        #[cfg(feature = "f128")]
        Box::new(QuadPrecisionTranslator {}),
    ];

    TranslatorList::new(
        basic_translators,
        numeric_translators,
        vec![
            Box::new(ClockTranslator::new()),
            Box::new(StringTranslator {}),
            Box::new(enum_translator::EnumTranslator {}),
        ],
    )
}

#[derive(Default)]
pub struct TranslatorList {
    inner: HashMap<String, AnyTranslator>,
    pub default: String,
}

impl TranslatorList {
    pub fn new(
        basic: Vec<Box<DynBasicTranslator>>,
        numeric: Vec<Box<DynNumericTranslator>>,
        translators: Vec<Box<DynTranslator>>,
    ) -> Self {
        Self {
            default: "Hexadecimal".to_string(),
            inner: basic
                .into_iter()
                .map(|t| (t.name(), AnyTranslator::Basic(t)))
                .chain(
                    numeric
                        .into_iter()
                        .map(|t| (t.name(), AnyTranslator::Numeric(t))),
                )
                .chain(
                    translators
                        .into_iter()
                        .map(|t| (t.name(), AnyTranslator::Full(t))),
                )
                .collect(),
        }
    }

    pub fn all_translator_names(&self) -> Vec<&String> {
        self.inner.keys().collect()
    }

    pub fn all_translators(&self) -> Vec<&AnyTranslator> {
        self.inner.values().collect()
    }

    pub fn basic_translator_names(&self) -> Vec<&String> {
        self.inner
            .iter()
            .filter_map(|(name, t)| t.is_basic().then_some(name))
            .collect()
    }

    pub fn get_translator(&self, name: &str) -> &AnyTranslator {
        self.inner
            .get(name)
            .unwrap_or_else(|| panic! {"No translator called {name}"})
    }

    pub fn add_or_replace(&mut self, t: AnyTranslator) {
        self.inner.insert(t.name(), t);
    }

    pub fn is_valid_translator(&self, meta: &VariableMeta, candidate: &str) -> bool {
        self.get_translator(candidate)
            .translates(meta)
            .map(|preference| preference != TranslationPreference::No)
            .unwrap_or(false)
    }
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
            let AnyTranslator::Basic(subtranslator) =
                translators.get_translator(subtranslator_name)
            else {
                panic!("Subtranslator '{subtranslator_name}' was not a basic translator");
            };

            Some(TranslatedValue::from_basic_translate(
                subtranslator.basic_translate(1, &VariableValue::String(val.to_string())),
            ))
        }
        ValueRepr::Bits(bit_count, bits) => {
            let AnyTranslator::Basic(subtranslator) =
                translators.get_translator(subtranslator_name)
            else {
                panic!("Subtranslator '{subtranslator_name}' was not a basic translator");
            };

            Some(TranslatedValue::from_basic_translate(
                subtranslator.basic_translate(*bit_count, &VariableValue::String(bits.clone())),
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

#[local_impl::local_impl]
impl TranslationResultExt for TranslationResult {
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
    fn format_flat(
        &self,
        root_format: &Option<String>,
        formats: &[crate::displayed_item::FieldFormat],
        translators: &TranslatorList,
    ) -> Vec<SubFieldFlatTranslationResult> {
        let sub_result = self.sub_format(formats, translators, &[]);

        // FIXME for consistency we should not fall back to `translators.default` here, but fetch the
        // preferred translator - but doing that ATM will break if the spade translator is used, since
        // on the first render the spade translator seems to not have loaded its information yet.
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
            ValueKind::Custom(custom_color) => *custom_color,
            ValueKind::Weak => theme.variable_weak,
            ValueKind::Normal => user_color,
        }
    }
}

pub struct StringTranslator {}

impl Translator<VarId, ScopeId, Message> for StringTranslator {
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
