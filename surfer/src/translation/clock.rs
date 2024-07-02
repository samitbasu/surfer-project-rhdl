use crate::wave_container::VariableMeta;
use surfer_translation_types::VariableValue;

use super::{BasicTranslator, BitTranslator, Translator, VariableInfo};

pub struct ClockTranslator {
    // In order to not duplicate logic, we'll re-use the bit translator internally
    inner: Box<dyn BasicTranslator>,
}

impl ClockTranslator {
    pub fn new() -> Self {
        Self {
            inner: Box::new(BitTranslator {}),
        }
    }
}

impl Translator for ClockTranslator {
    fn name(&self) -> String {
        "Clock".to_string()
    }

    fn translate(
        &self,
        variable: &VariableMeta,
        value: &VariableValue,
    ) -> color_eyre::Result<super::TranslationResult> {
        self.inner.translate(variable, value)
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
