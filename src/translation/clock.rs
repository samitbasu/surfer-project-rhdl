use color_eyre::eyre::anyhow;
use fastwave_backend::SignalValue;

use crate::wave_container::SignalMeta;

use super::{BasicTranslator, BitTranslator, SignalInfo, Translator};

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
        signal: &SignalMeta,
        value: &SignalValue,
    ) -> color_eyre::Result<super::TranslationResult> {
        if signal.num_bits == Some(1) {
            self.inner.translate(signal, value)
        } else {
            Err(anyhow!(
                "Clock translator translates a signal which is not 1 bit wide"
            ))
        }
    }

    fn signal_info(&self, _signal: &SignalMeta) -> color_eyre::Result<super::SignalInfo> {
        Ok(SignalInfo::Clock)
    }

    fn translates(&self, signal: &SignalMeta) -> color_eyre::Result<super::TranslationPreference> {
        if signal.num_bits == Some(1) {
            Ok(super::TranslationPreference::Yes)
        } else {
            Ok(super::TranslationPreference::No)
        }
    }
}
