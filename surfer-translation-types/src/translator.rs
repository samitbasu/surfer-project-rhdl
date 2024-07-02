use color_eyre::Result;
use num::BigUint;

use crate::{
    TranslationPreference, ValueKind, VariableEncoding, VariableInfo, VariableMeta, VariableValue,
};

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
