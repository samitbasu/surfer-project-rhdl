use color_eyre::Result;
use fastwave_backend::SignalValue;
use num::BigUint;

use crate::wave_container::SignalMeta;

use super::{
    map_vector_signal, translates_all_bit_types, BasicTranslator, NumberParseResult,
    TranslationPreference, ValueKind,
};

pub trait NumericTranslator {
    fn name(&self) -> String;
    fn translate_biguint(&self, _: u64, _: BigUint) -> String;
    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        translates_all_bit_types(signal)
    }
}

impl<T: NumericTranslator> BasicTranslator for T {
    fn name(&self) -> String {
        self.name()
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => (
                self.translate_biguint(num_bits, v.clone()),
                ValueKind::Normal,
            ),
            SignalValue::String(s) => match map_vector_signal(s) {
                NumberParseResult::Unparsable(v, k) => (v, k),
                NumberParseResult::Numerical(v) => {
                    (self.translate_biguint(num_bits, v), ValueKind::Normal)
                }
            },
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        self.translates(signal)
    }
}
