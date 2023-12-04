use std::collections::{BTreeMap, HashMap};

use color_eyre::{eyre::anyhow, Result};
use fastwave_backend::{ScopeIdx, Signal, SignalErrors, SignalIdx, SignalValue, VCD};
use log::warn;
use num::BigUint;

use crate::wave_container::{ModuleRef, SignalRef};

#[derive(Debug)]
pub struct FastWaveContainer {
    pub inner: VCD,
    pub signals: Vec<SignalRef>,
    pub module_map: HashMap<ModuleRef, ScopeIdx>,
    pub inv_module_map: HashMap<ScopeIdx, ModuleRef>,
    /// Mapping from signal ref to the signal index. Aliases are already resolved so
    /// the index should be a *real* signal. This means that querying the name on the
    /// FWB signal may be wrong.
    pub signal_map: BTreeMap<SignalRef, SignalIdx>,
    /// (Almost) reverse mapping from signal_map, though here aliases are not resolved.
    pub inv_signal_map: HashMap<SignalIdx, SignalRef>,
}

impl FastWaveContainer {
    pub fn new(vcd: VCD) -> Self {
        let mut result = Self {
            inner: vcd,
            signals: vec![],
            module_map: HashMap::new(),
            inv_module_map: HashMap::new(),
            signal_map: BTreeMap::new(),
            inv_signal_map: HashMap::new(),
        };
        result.initialize_signal_scope_maps();
        result
    }

    fn get_signal_idx(&self, signal_ref: &SignalRef) -> Result<SignalIdx> {
        self.signal_map
            .get(signal_ref)
            .copied()
            .ok_or_else(|| anyhow!("No FWB signal index for {}", signal_ref.full_path_string()))
    }

    pub fn signals(&self) -> &Vec<SignalRef> {
        &self.signals
    }

    pub fn signals_in_module(&self, module: &ModuleRef) -> Vec<SignalRef> {
        let Some(module_idx) = self.module_map.get(module) else {
            warn!("Found no module '{module}'. Defaulting to no signals");
            return vec![];
        };

        self.inner
            .get_children_signal_idxs(*module_idx)
            .into_iter()
            .map(|idx| self.inv_signal_map[&idx].clone())
            .collect()
    }

    pub fn signal_exists(&self, signal: &SignalRef) -> bool {
        let Some(module_idx) = self.module_map.get(&signal.path) else {
            return false;
        };
        self.inner
            .get_children_signal_idxs(*module_idx)
            .iter()
            .any(|idx| self.inv_signal_map[idx].name == signal.name)
    }

    pub fn fwb_signal(&self, signal: &SignalRef) -> Result<Signal> {
        self.get_signal_idx(signal)
            .map(|idx| self.inner.signal_from_signal_idx(idx))
    }

    pub fn query_signal(
        &self,
        signal: &SignalRef,
        time: &BigUint,
    ) -> Result<Option<(BigUint, SignalValue)>> {
        let idx = self.get_signal_idx(signal)?;
        match self
            .inner
            .signal_from_signal_idx(idx)
            .query_val_on_tmln(time, &self.inner)
        {
            Ok(val) => Ok(Some(val)),
            Err(SignalErrors::PreTimeline { .. } | SignalErrors::EmptyTimeline) => Ok(None),
            Err(other) => Err(anyhow!("Fast wave error {other:?}")),
        }
    }

    pub fn modules(&self) -> Vec<ModuleRef> {
        // NOTE: If performance turns into an issue, we could also cache these, but keeping
        // state around is annoying
        self.module_map.keys().cloned().collect()
    }

    pub fn root_modules(&self) -> Vec<ModuleRef> {
        self.inner
            .root_scopes_by_idx()
            .iter()
            .map(|idx| self.inv_module_map[idx].clone())
            .collect()
    }

    pub fn child_modules(&self, module: &ModuleRef) -> Result<Vec<ModuleRef>> {
        let idx = self
            .module_map
            .get(module)
            .ok_or_else(|| anyhow!("Failed to get ScopeIdx for {module}"))?;

        Ok(self
            .inner
            .child_scopes_by_idx(*idx)
            .iter()
            .map(|id| self.inv_module_map[id].clone())
            .collect())
    }

    pub fn module_exists(&self, module: &ModuleRef) -> bool {
        self.module_map.get(module).is_some()
    }

    // Initializes the scopes_to_ids and signals_to_ids
    // fields by iterating down the scope hierarchy and collectiong
    // the absolute names of all signals and scopes
    fn initialize_signal_scope_maps(&mut self) {
        for root_scope in self.inner.root_scopes_by_idx() {
            self.add_scope_signals(root_scope, &ModuleRef::from_strs(&[]));
        }
    }

    // in scope S and path P, adds all signals x to all_signal_names
    // as [S.]P.x
    // does the same for scopes
    // goes down into subscopes and does the same there
    fn add_scope_signals(&mut self, scope: ScopeIdx, parent_module: &ModuleRef) {
        let scope_name = self.inner.scope_name_by_idx(scope).clone();
        let full_scope = parent_module.with_submodule(scope_name);
        self.module_map.insert(full_scope.clone(), scope);
        self.inv_module_map.insert(scope, full_scope.clone());

        let signal_idxs = self.inner.get_children_signal_idxs(scope);
        for unresolved_signal_idx in signal_idxs {
            let sig = self.inner.signal_from_signal_idx(unresolved_signal_idx);
            let signal_name = sig.name();
            let resolved_idx = sig.real_idx();
            // FIXME configurable filter
            // if !signal_name.starts_with('_') {
            let signal_ref = SignalRef::new(full_scope.clone(), signal_name);
            self.signals.push(signal_ref.clone());
            self.signal_map.insert(signal_ref.clone(), resolved_idx);
            self.inv_signal_map
                .insert(unresolved_signal_idx, signal_ref);
            // }
        }

        for sub_scope in self.inner.child_scopes_by_idx(scope) {
            self.add_scope_signals(sub_scope, &full_scope);
        }
    }
}
