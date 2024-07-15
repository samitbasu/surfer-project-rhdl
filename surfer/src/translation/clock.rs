use surfer_translation_types::{TranslationResult, Translator, VariableInfo, VariableValue};

use crate::message::Message;
use crate::translation::{AnyTranslator, BitTranslator};
use crate::wave_container::{ScopeId, VarId, VariableMeta};

pub struct ClockTranslator {
    // In order to not duplicate logic, we'll re-use the bit translator internally
    inner: AnyTranslator,
}

impl ClockTranslator {
    pub fn new() -> Self {
        Self {
            inner: AnyTranslator::Basic(Box::new(BitTranslator {})),
        }
    }
}

impl Translator<VarId, ScopeId, Message> for ClockTranslator {
    fn name(&self) -> String {
        "Clock".to_string()
    }

    fn translate(
        &self,
        variable: &surfer_translation_types::VariableMeta<VarId, ScopeId>,
        value: &VariableValue,
    ) -> color_eyre::Result<TranslationResult> {
        self.inner.translate(variable, value)
    }

    fn variable_info(&self, _variable: &VariableMeta) -> color_eyre::Result<VariableInfo> {
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
