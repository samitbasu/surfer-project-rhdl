//! Definition of the main [`Translator`] type and the simplified versions
//! [`BasicTranslator`] and [`NumericTranslator`].
use crate::result::ValueRepr;
use color_eyre::Result;
use num::BigUint;
use std::sync::mpsc::Sender;

use crate::result::TranslationResult;
use crate::{
    TranslationPreference, ValueKind, VariableEncoding, VariableInfo, VariableMeta, VariableValue,
};

/// The most general translator type.
pub trait Translator<VarId, ScopeId, Message>: Send + Sync {
    fn name(&self) -> String;

    fn translate(
        &self,
        variable: &VariableMeta<VarId, ScopeId>,
        value: &VariableValue,
    ) -> Result<TranslationResult>;

    fn variable_info(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<VariableInfo>;

    /// Return [`TranslationPreference`] based on if the translator can handle this variable.
    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference>;

    /// By default translators are stateless, but if they need to reload, they can
    /// do by defining this method.
    /// Long running translators should run the reloading in the background using `perform_work`
    fn reload(&self, _sender: Sender<Message>) {}
}

impl<VarId, ScopeId, Message> Translator<VarId, ScopeId, Message>
    for Box<dyn BasicTranslator<VarId, ScopeId>>
{
    fn name(&self) -> String {
        self.as_ref().name()
    }

    fn translate(
        &self,
        variable: &VariableMeta<VarId, ScopeId>,
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

    fn variable_info(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<VariableInfo> {
        self.as_ref().variable_info(variable)
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        self.as_ref().translates(variable)
    }
}

/// Simplified translator.
pub trait BasicTranslator<VarId, ScopeId>: Send + Sync {
    fn name(&self) -> String;
    fn basic_translate(&self, num_bits: u64, value: &VariableValue) -> (String, ValueKind);
    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        translates_all_bit_types(variable)
    }
    fn variable_info(&self, _variable: &VariableMeta<VarId, ScopeId>) -> Result<VariableInfo> {
        Ok(VariableInfo::Bits)
    }
}

/// Simplified translator that only handles vectors with 0 and 1 (other values are handled by the trait).
/// This is handled by defining the method [`NumericTranslator::translate_biguint`].
pub trait NumericTranslator<VarId, ScopeId> {
    fn name(&self) -> String;
    fn translate_biguint(&self, _: u64, _: BigUint) -> String;
    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        translates_all_bit_types(variable)
    }
}

impl<T: NumericTranslator<VarId, ScopeId> + Send + Sync, VarId, ScopeId>
    BasicTranslator<VarId, ScopeId> for T
{
    fn name(&self) -> String {
        self.name()
    }

    fn basic_translate(&self, num_bits: u64, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => (
                self.translate_biguint(num_bits, v.clone()),
                ValueKind::Normal,
            ),
            VariableValue::String(s) => match map_vector_variable(s) {
                NumberParseResult::Unparsable(v, k) => (v, k),
                NumberParseResult::Numerical(v) => {
                    (self.translate_biguint(num_bits, v), ValueKind::Normal)
                }
            },
        }
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        self.translates(variable)
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

pub fn translates_all_bit_types<VarId, ScopeId>(
    variable: &VariableMeta<VarId, ScopeId>,
) -> Result<TranslationPreference> {
    if variable.encoding == VariableEncoding::BitVector {
        Ok(TranslationPreference::Yes)
    } else {
        Ok(TranslationPreference::No)
    }
}
