use std::collections::HashMap;

use camino::Utf8Path;
use spade::compiler_state::CompilerState;

use color_eyre::{
    eyre::{anyhow, Context},
    Result,
};
use spade_common::{
    location_info::WithLocation,
    name::{Identifier, Path, NameID},
};
use vcd_translate::translation::{inner_translate_value, value_from_str};

use super::{Translator, TranslationResult, ValueRepr, SignalInfo};

pub struct SpadeTranslator {
    state: CompilerState,
    top: NameID,
}

impl SpadeTranslator {
    pub fn new(top: &str, state_file: &Utf8Path) -> Result<Self> {
        let file_content = std::fs::read_to_string(&state_file)
            .with_context(|| format!("Failed to read {state_file}"))?;

        let state: CompilerState = ron::from_str(&file_content)
            .with_context(|| format!("Failed to decode {state_file}"))?;

        let path = top.split("::").map(|s| Identifier(s.to_string()).nowhere());
        let (top, _) = state
            .symtab
            .symtab()
            .lookup_unit(&Path(path.collect()).nowhere())
            .map_err(|_| anyhow!("Did not find a unit {top} in {state_file}"))?;

        Ok(Self { state, top })
    }
}

impl Translator for SpadeTranslator {
    fn name(&self) -> String {
        "spade".to_string()
    }

    fn translate(
        &self,
        signal: &fastwave_backend::Signal,
        value: &fastwave_backend::SignalValue,
    ) -> Result<TranslationResult> {
        let ty = self.state.type_of_hierarchical_value(&self.top, &signal.path()[1..])?;

        let mut result = String::new();
        let val_vcd = match value {
            fastwave_backend::SignalValue::BigUint(v) => value_from_str(&format!("{v:b}")),
            fastwave_backend::SignalValue::String(v) => value_from_str(v),
        };
        inner_translate_value(&mut result, &val_vcd, &ty);
        Ok(TranslationResult {
            val: ValueRepr::String(result),
            subfields: vec![],
            durations: HashMap::new(),
        })
    }

    fn signal_info(
        &self,
        signal: &fastwave_backend::Signal,
        _name: &str,
    ) -> Result<SignalInfo> {
        Ok(SignalInfo::Bits)
    }
}
