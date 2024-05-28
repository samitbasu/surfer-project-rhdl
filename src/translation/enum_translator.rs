use crate::translation::{
    TranslationPreference, TranslationResult, Translator, ValueKind, ValueRepr, VariableInfo,
};
use crate::wave_container::{VariableMeta, VariableValue};
use color_eyre::Result;
use std::borrow::Cow;

pub struct EnumTranslator {}

impl Translator for EnumTranslator {
    fn name(&self) -> String {
        "Enum".to_string()
    }

    fn translate(&self, meta: &VariableMeta, value: &VariableValue) -> Result<TranslationResult> {
        let str_value = match value {
            VariableValue::BigUint(v) => Cow::Owned(format!(
                "{v:0width$b}",
                width = meta.num_bits.unwrap() as usize
            )),
            VariableValue::String(s) => Cow::Borrowed(s),
        };
        let (kind, name) = meta
            .enum_map
            .get(str_value.as_str())
            .map(|s| (ValueKind::Normal, s.to_string()))
            .unwrap_or((ValueKind::Warn, format!("ERROR ({str_value})")));
        Ok(TranslationResult {
            val: ValueRepr::String(name),
            kind,
            subfields: vec![],
        })
    }

    fn variable_info(&self, _variable: &VariableMeta) -> color_eyre::Result<VariableInfo> {
        Ok(VariableInfo::Bits)
    }

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        if variable.enum_map.is_empty() {
            Ok(TranslationPreference::No)
        } else {
            Ok(TranslationPreference::Prefer)
        }
    }
}
