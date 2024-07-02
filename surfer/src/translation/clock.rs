use crate::message::Message;
use crate::wave_container::{ScopeId, VarId, VariableMeta};
use surfer_translation_types::{TranslationResult, Translator, VariableValue};

use super::{BitTranslator, DynBasicTranslator, DynTranslator, VariableInfo};

pub struct ClockTranslator {
    // In order to not duplicate logic, we'll re-use the bit translator internally
    inner: Box<DynBasicTranslator>,
}

impl ClockTranslator {
    pub fn new() -> Self {
        Self {
            inner: Box::new(BitTranslator {}),
        }
    }
}

impl Translator<VarId, ScopeId, Message> for ClockTranslator {
    fn name(&self) -> String {
        "Clock".to_string()
    }

    fn translate(
        &self,
        variable: &VariableMeta,
        value: &VariableValue,
    ) -> color_eyre::Result<TranslationResult> {
        (&self.inner as &DynTranslator).translate(variable, value)
    }

    fn variable_info(&self, _variable: &VariableMeta) -> color_eyre::Result<super::VariableInfo> {
        Ok(VariableInfo::Clock)
    }

    fn translates(
        &self,
        variable: &VariableMeta,
    ) -> color_eyre::Result<super::TranslationPreference> {
        if variable.num_bits == Some(1) {
            Ok(super::TranslationPreference::Yes)
        } else {
            Ok(super::TranslationPreference::No)
        }
    }
}
