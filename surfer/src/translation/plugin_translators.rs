use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use color_eyre::eyre::anyhow;
use extism::{Manifest, Plugin, Wasm};
use surfer_translation_types::plugin_types::TranslateParams;
use surfer_translation_types::{
    PluginConfig, TranslationPreference, TranslationResult, Translator, VariableInfo, VariableMeta,
    VariableValue,
};

use crate::message::Message;
use crate::wave_container::{ScopeId, VarId};

pub struct PluginTranslator {
    plugin: Arc<Mutex<Plugin>>,
}

impl PluginTranslator {
    pub fn new(data: &[u8], config: HashMap<String, String>) -> Self {
        let manifest = Manifest::new([Wasm::data(data)]);
        let mut plugin = Plugin::new(manifest, [], true).unwrap();

        plugin.call::<_, ()>("new", PluginConfig(config)).unwrap();
        Self {
            plugin: Arc::new(Mutex::new(plugin)),
        }
    }
}

impl Translator<VarId, ScopeId, Message> for PluginTranslator {
    fn name(&self) -> String {
        self.plugin
            .lock()
            .unwrap()
            .call::<_, &str>("name", ())
            .unwrap()
            .to_string()
    }

    fn translate(
        &self,
        variable: &VariableMeta<VarId, ScopeId>,
        value: &VariableValue,
    ) -> color_eyre::Result<TranslationResult> {
        let result = self
            .plugin
            .lock()
            .unwrap()
            .call(
                "translate",
                TranslateParams {
                    variable: variable.clone().map_ids(|_| (), |_| ()),
                    value: value.clone(),
                },
            )
            .unwrap();
        Ok(result)
    }

    fn variable_info(
        &self,
        variable: &VariableMeta<VarId, ScopeId>,
    ) -> color_eyre::Result<VariableInfo> {
        let result = self
            .plugin
            .lock()
            .unwrap()
            .call("variable_info", variable.clone().map_ids(|_| (), |_| ()))
            .unwrap();
        Ok(result)
    }

    fn translates(
        &self,
        variable: &VariableMeta<VarId, ScopeId>,
    ) -> color_eyre::Result<TranslationPreference> {
        match self
            .plugin
            .lock()
            .unwrap()
            .call("translates", variable.clone().map_ids(|_| (), |_| ()))
        {
            Ok(r) => Ok(r),
            Err(e) => Err(anyhow!(e)),
        }
    }
}
