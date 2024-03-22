use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

use chrono::prelude::{DateTime, Utc};
use color_eyre::{eyre::bail, Result};
use num::BigUint;
use serde::{Deserialize, Serialize};
use wellen::{self, VarRef};

#[cfg(not(target_arch = "wasm32"))]
use crate::cxxrtl_container::CxxrtlContainer;
use crate::wellen::var_to_meta;
use crate::{
    time::{TimeScale, TimeUnit},
    variable_type::VariableType,
    wellen::WellenContainer,
};

#[derive(Debug, Clone)]
pub enum SimulationStatus {
    Paused,
    Running,
    Finished,
}

#[derive(Debug, PartialEq, Clone)]
pub enum VariableValue {
    BigUint(BigUint),
    String(String),
}

pub struct MetaData {
    pub date: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub timescale: TimeScale,
}
#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct ScopeRef {
    pub(crate) strs: Vec<String>,
    /// Backend specific numeric ID. Performance optimization.
    #[serde(skip, default = "__wave_container_scope_id_none")]
    pub(crate) id: WaveContainerScopeId,
}

impl std::hash::Hash for ScopeRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // id is intentionally not hashed, since it is only a performance hint
        self.strs.hash(state)
    }
}

impl PartialEq for ScopeRef {
    fn eq(&self, other: &Self) -> bool {
        // id is intentionally not compared, since it is only a performance hint
        self.strs.eq(&other.strs)
    }
}

/// A backend-specific, numeric reference for fast access to the associated scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WaveContainerScopeId {
    None,
    Wellen(wellen::ScopeRef),
}

fn __wave_container_scope_id_none() -> WaveContainerScopeId {
    WaveContainerScopeId::None
}

impl ScopeRef {
    pub fn empty() -> Self {
        Self {
            strs: vec![],
            id: WaveContainerScopeId::None,
        }
    }

    pub fn from_strs<S: ToString>(s: &[S]) -> Self {
        let strs = s.iter().map(|s| s.to_string()).collect();
        let id = WaveContainerScopeId::None;
        Self { strs, id }
    }

    pub fn from_strs_with_wellen_id<S: ToString>(s: &[S], id: wellen::ScopeRef) -> Self {
        let mut a = Self::from_strs(s);
        a.id = WaveContainerScopeId::Wellen(id);
        a
    }

    /// Creates a ScopeRef from a string with each scope separated by `.`
    pub fn from_hierarchy_string(s: &str) -> Self {
        let strs = s.split('.').map(|x| x.to_string()).collect();
        let id = WaveContainerScopeId::None;
        Self { strs, id }
    }

    pub fn with_subscope(&self, subscope: String) -> Self {
        let mut result = self.clone();
        result.strs.push(subscope);
        // the result refers to a different scope, which we do not know the ID of
        result.id = WaveContainerScopeId::None;
        result
    }

    pub(crate) fn name(&self) -> String {
        self.strs.last().cloned().unwrap_or_default()
    }

    pub(crate) fn strs(&self) -> &[String] {
        &self.strs
    }

    pub(crate) fn get_wellen_id(&self) -> Option<wellen::ScopeRef> {
        match self.id {
            WaveContainerScopeId::Wellen(id) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn with_wellen_id(&self, id: wellen::ScopeRef) -> Self {
        let mut out = self.clone();
        out.id = WaveContainerScopeId::Wellen(id);
        out
    }
}

impl std::fmt::Display for ScopeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.strs.join("."))
    }
}

// FIXME: We'll be cloning these quite a bit, I wonder if a `Cow<&str>` or Rc/Arc would be better
#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct VariableRef {
    /// Path in the scope hierarchy to where this variable resides
    pub path: ScopeRef,
    /// Name of the variable in its hierarchy
    pub name: String,
    /// Backend specific numeric ID. Performance optimization.
    #[serde(skip, default = "__wave_container_var_id_none")]
    pub(crate) id: WaveContainerVarId,
}

impl AsRef<VariableRef> for VariableRef {
    fn as_ref(&self) -> &VariableRef {
        self
    }
}

impl std::hash::Hash for VariableRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // id is intentionally not hashed, since it is only a performance hint
        self.path.hash(state);
        self.name.hash(state);
    }
}

impl std::cmp::PartialEq for VariableRef {
    fn eq(&self, other: &Self) -> bool {
        // id is intentionally not compared, since it is only a performance hint
        self.path.eq(&other.path) && self.name.eq(&other.name)
    }
}

/// A backend-specific, numeric reference for fast access to the associated variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WaveContainerVarId {
    None,
    Wellen(wellen::VarRef),
}

fn __wave_container_var_id_none() -> WaveContainerVarId {
    WaveContainerVarId::None
}

impl VariableRef {
    pub fn new(path: ScopeRef, name: String) -> Self {
        Self {
            path,
            name,
            id: WaveContainerVarId::None,
        }
    }

    pub(crate) fn new_with_wave_id(path: ScopeRef, name: String, id: VarRef) -> Self {
        Self {
            path,
            name,
            id: WaveContainerVarId::Wellen(id),
        }
    }

    pub fn from_hierarchy_string(s: &str) -> Self {
        let components = s.split('.').map(|s| s.to_string()).collect::<Vec<_>>();

        if components.is_empty() {
            Self {
                path: ScopeRef::empty(),
                name: String::new(),
                id: WaveContainerVarId::None,
            }
        } else {
            Self {
                path: ScopeRef::from_strs(&components[..(components.len()) - 1]),
                name: components.last().unwrap().to_string(),
                id: WaveContainerVarId::None,
            }
        }
    }

    /// A human readable full path to the scope
    pub fn full_path_string(&self) -> String {
        if self.path.strs().is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.path, self.name)
        }
    }

    pub fn full_path(&self) -> Vec<String> {
        self.path
            .strs()
            .iter()
            .cloned()
            .chain([self.name.clone()])
            .collect()
    }

    #[cfg(test)]
    pub fn from_strs(s: &[&str]) -> Self {
        Self {
            path: ScopeRef::from_strs(&s[..(s.len() - 1)]),
            name: s
                .last()
                .expect("from_strs called with an empty string")
                .to_string(),
            id: WaveContainerVarId::None,
        }
    }

    pub(crate) fn get_wellen_id(&self) -> Option<VarRef> {
        match self.id {
            WaveContainerVarId::Wellen(id) => Some(id),
            _ => None,
        }
    }

    /// Removes any backend specific ID.
    pub(crate) fn clear_id(&mut self) {
        self.id = WaveContainerVarId::None;
    }
}

/// A reference to a field of a larger variable, such as a field in a struct. The fields
/// are the recursive path to the fields inside the (translated) root
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRef {
    pub root: VariableRef,
    pub field: Vec<String>,
}

impl FieldRef {
    pub fn without_fields(root: VariableRef) -> Self {
        Self {
            root,
            field: vec![],
        }
    }

    #[cfg(test)]
    pub fn from_strs(root: &[&str], field: &[&str]) -> Self {
        Self {
            root: VariableRef::from_strs(root),
            field: field.into_iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Debug, Default)]
pub struct QueryResult {
    pub current: Option<(BigUint, VariableValue)>,
    pub next: Option<BigUint>,
}

pub enum WaveContainer {
    Wellen(WellenContainer),
    /// A wave container that contains nothing. Currently, the only practical use for this is
    /// a placehodler when serializing and deserializing wave state.
    Empty,
    #[cfg(not(target_arch = "wasm32"))]
    Cxxrtl(Mutex<CxxrtlContainer>),
}

impl WaveContainer {
    pub fn new_waveform(hierarchy: std::sync::Arc<wellen::Hierarchy>) -> Self {
        WaveContainer::Wellen(WellenContainer::new(hierarchy))
    }

    /// Creates a new empty wave container. Should only be used as a default for serde. If
    /// no wave container is present, the WaveData should be None, rather than this being
    /// Empty
    pub fn __new_empty() -> Self {
        WaveContainer::Empty
    }

    pub fn wants_anti_aliasing(&self) -> bool {
        match self {
            WaveContainer::Wellen(_) => true,
            WaveContainer::Empty => true,
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(_) => false,
        }
    }

    /// Returns true if all requested signals have been loaded.
    /// Used for testing to make sure the GUI is at its final state before taking a
    /// snapshot.
    pub fn is_fully_loaded(&self) -> bool {
        match self {
            WaveContainer::Wellen(f) => f.is_fully_loaded(),
            WaveContainer::Empty => true,
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(_) => true,
        }
    }

    /// Returns the full names of all variables in the design.
    pub fn variable_names(&self) -> Vec<String> {
        match self {
            WaveContainer::Wellen(f) => f.variable_names(),
            WaveContainer::Empty => vec![],
            // I don't know if we can do
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(_) => vec![], // FIXME: List signals
        }
    }

    pub fn variables(&self) -> Vec<VariableRef> {
        match self {
            WaveContainer::Wellen(f) => f.variables(),
            WaveContainer::Empty => vec![],
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(_) => vec![],
        }
    }

    pub fn variables_in_scope(&self, scope: &ScopeRef) -> Vec<VariableRef> {
        match self {
            WaveContainer::Wellen(f) => f.variables_in_scope(scope),
            WaveContainer::Empty => vec![],
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().signals_in_module(scope),
        }
    }

    /// Loads multiple variables at once. This is useful when we want to add multiple variables in one go.
    pub fn load_variables<S: AsRef<VariableRef>, T: Iterator<Item = S>>(
        &mut self,
        variables: T,
    ) -> Result<()> {
        match self {
            WaveContainer::Wellen(f) => f.load_variables(variables),
            WaveContainer::Empty => bail!("Cannot load variables from empty container."),
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => Ok(c.lock().unwrap().load_signals(variables)),
        }
    }

    pub fn variable_meta<'a>(&'a self, r: &'a VariableRef) -> Result<VariableMeta> {
        match self {
            WaveContainer::Wellen(f) => {
                let var = f.get_var(r)?;
                Ok(var_to_meta(var, f.get_enum_map(var), r))
            }
            WaveContainer::Empty => bail!("Getting meta from empty wave container"),
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().signal_meta(r),
        }
    }

    /// Query the value of the variable at a certain time step.
    /// Returns `None` if we do not have any values for the variable.
    /// That generally happens if the corresponding signal is still being loaded.
    pub fn query_variable(
        &self,
        variable: &VariableRef,
        time: &BigUint,
    ) -> Result<Option<QueryResult>> {
        match self {
            WaveContainer::Wellen(f) => f.query_variable(variable, time),
            WaveContainer::Empty => bail!("Querying variable from empty wave container"),
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => Ok(c.lock().unwrap().query_signal(variable, time)),
        }
    }

    /// Looks up the variable _by name_ and returns a new reference with an updated `id` if the variable is found.
    pub fn update_variable_ref(&self, variable: &VariableRef) -> Option<VariableRef> {
        match self {
            WaveContainer::Wellen(f) => f.update_variable_ref(variable),
            WaveContainer::Empty => None,
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(_) => None,
        }
    }

    /// Returns the full names of all scopes in the design.
    pub fn scope_names(&self) -> Vec<String> {
        match self {
            WaveContainer::Wellen(f) => f.scope_names(),
            WaveContainer::Empty => vec![],
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c
                .lock()
                .unwrap()
                .modules()
                .iter()
                .map(|m| m.strs().last().cloned().unwrap_or(format!("root")))
                .collect(),
        }
    }

    pub fn metadata(&self) -> MetaData {
        match self {
            WaveContainer::Wellen(f) => f.metadata(),
            WaveContainer::Empty => MetaData {
                date: None,
                version: None,
                timescale: TimeScale {
                    unit: TimeUnit::None,
                    multiplier: None,
                },
            },
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(_) => {
                MetaData {
                    date: None,
                    version: None,
                    timescale: TimeScale {
                        // Cxxrtl always uses FemtoSeconds
                        unit: TimeUnit::FemtoSeconds,
                        multiplier: None,
                    },
                }
            }
        }
    }

    pub fn root_scopes(&self) -> Vec<ScopeRef> {
        match self {
            WaveContainer::Wellen(f) => f.root_scopes(),
            WaveContainer::Empty => vec![],
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().root_modules(),
        }
    }

    pub fn child_scopes(&self, scope: &ScopeRef) -> Result<Vec<ScopeRef>> {
        match self {
            WaveContainer::Wellen(f) => f.child_scopes(scope),
            WaveContainer::Empty => bail!("Getting child modules from empty wave container"),
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => Ok(c.lock().unwrap().child_scopes(scope)),
        }
    }

    pub fn max_timestamp(&self) -> Option<BigUint> {
        match self {
            WaveContainer::Wellen(f) => f.max_timestamp(),
            WaveContainer::Empty => None,
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c
                .lock()
                .unwrap()
                .max_timestamp()
                .map(|t| t.into_femtoseconds()),
        }
    }

    pub fn scope_exists(&self, scope: &ScopeRef) -> bool {
        match self {
            WaveContainer::Wellen(f) => f.scope_exists(scope),
            WaveContainer::Empty => false,
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().module_exists(scope),
        }
    }

    /// Returns a human readable string with information about a scope.
    /// The scope name itself should not be included, since it will be prepended automatically.
    pub fn get_scope_tooltip_data(&self, scope: &ScopeRef) -> String {
        match self {
            WaveContainer::Wellen(f) => f.get_scope_tooltip_data(scope),
            WaveContainer::Empty => "".to_string(),
            // FIXME: Tooltip
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(_) => "".to_string(),
        }
    }

    /// Returns the simulation status for this wave source if it exists. Wave sources which have no
    /// simulation status should return None here, otherwise buttons for controlling simulation
    /// will be shown
    pub fn simulation_status(&self) -> Option<SimulationStatus> {
        match self {
            WaveContainer::Wellen(_) => None,
            WaveContainer::Empty => None,
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().simulation_status(),
        }
    }

    /// If [simulation_status] is `Some(SimulationStatus::Paused)`, attempt to unpause the
    /// simulation otherwise does nothing
    pub fn unpause_simulation(&self) {
        match self {
            WaveContainer::Wellen(_) => {}
            WaveContainer::Empty => {}
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().unpause(),
        }
    }

    /// See [unpause_simulation]
    pub fn pause_simulation(&self) {
        match self {
            WaveContainer::Wellen(_) => {}
            WaveContainer::Empty => {}
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().pause(),
        }
    }

    /// Called for `wellen` container, when the body of the waveform file has been parsed.
    pub fn wellen_add_body(
        &mut self,
        time_table: wellen::TimeTable,
        source: wellen::SignalSource,
    ) -> Result<()> {
        match self {
            WaveContainer::Wellen(inner) => inner.add_body(time_table, source),
            _ => {
                bail!("Should never call this function on a non wellen container!")
            }
        }
    }
}

pub struct VariableMeta {
    pub var: VariableRef,
    pub num_bits: Option<u32>,
    pub variable_type: Option<VariableType>,
    pub index: Option<String>,
    pub enum_map: HashMap<String, String>,
}
