#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

use chrono::prelude::{DateTime, Utc};
use color_eyre::{eyre::bail, Result};
use num::BigUint;
use surfer_translation_types::VariableValue;

#[cfg(not(target_arch = "wasm32"))]
use crate::cxxrtl_container::CxxrtlContainer;
use crate::message::BodyResult;
use crate::time::{TimeScale, TimeUnit};
use crate::wellen::{var_to_meta, LoadSignalsCmd, LoadSignalsResult, WellenContainer};

pub type FieldRef = surfer_translation_types::FieldRef<VarId, ScopeId>;
pub type ScopeRef = surfer_translation_types::ScopeRef<ScopeId>;
pub type VariableRef = surfer_translation_types::VariableRef<VarId, ScopeId>;
pub type VariableMeta = surfer_translation_types::VariableMeta<VarId, ScopeId>;

#[derive(Debug, Clone)]
pub enum SimulationStatus {
    Paused,
    Running,
    Finished,
}

pub struct MetaData {
    pub date: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub timescale: TimeScale,
}

/// A backend-specific, numeric reference for fast access to the associated scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopeId {
    None,
    Wellen(wellen::ScopeRef),
}

impl Default for ScopeId {
    fn default() -> Self {
        Self::None
    }
}

/// A backend-specific, numeric reference for fast access to the associated variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VarId {
    None,
    Wellen(wellen::VarRef),
}

impl Default for VarId {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Default)]
pub struct QueryResult {
    pub current: Option<(BigUint, VariableValue)>,
    pub next: Option<BigUint>,
}

#[local_impl::local_impl]
impl ScopeRefExt for ScopeRef {
    fn empty() -> Self {
        Self {
            strs: vec![],
            id: ScopeId::default(),
        }
    }

    fn from_strs<S: ToString>(s: &[S]) -> Self {
        Self::from_strs_with_id(s, ScopeId::default())
    }

    fn from_strs_with_id(s: &[impl ToString], id: ScopeId) -> Self {
        let strs = s.iter().map(|s| s.to_string()).collect();
        Self { strs, id }
    }

    /// Creates a ScopeRef from a string with each scope separated by `.`
    fn from_hierarchy_string(s: &str) -> Self {
        let strs = s.split('.').map(|x| x.to_string()).collect();
        let id = ScopeId::default();
        Self { strs, id }
    }

    fn with_subscope(&self, subscope: String) -> Self {
        let mut result = self.clone();
        result.strs.push(subscope);
        // the result refers to a different scope, which we do not know the ID of
        result.id = ScopeId::default();
        result
    }

    fn name(&self) -> String {
        self.strs.last().cloned().unwrap_or_default()
    }

    fn strs(&self) -> &[String] {
        &self.strs
    }

    fn with_id(&self, id: ScopeId) -> Self {
        let mut out = self.clone();
        out.id = id;
        out
    }

    fn cxxrtl_repr(&self) -> String {
        self.strs.join(" ")
    }
}

#[local_impl::local_impl]
impl VariableRefExt for VariableRef {
    fn new(path: ScopeRef, name: String) -> Self {
        Self::new_with_id(path, name, VarId::default())
    }

    fn new_with_id(path: ScopeRef, name: String, id: VarId) -> Self {
        Self { path, name, id }
    }

    fn from_hierarchy_string(s: &str) -> Self {
        let components = s.split('.').map(|s| s.to_string()).collect::<Vec<_>>();

        if components.is_empty() {
            Self {
                path: ScopeRef::empty(),
                name: String::new(),
                id: VarId::default(),
            }
        } else {
            Self {
                path: ScopeRef::from_strs(&components[..(components.len()) - 1]),
                name: components.last().unwrap().to_string(),
                id: VarId::default(),
            }
        }
    }

    /// A human readable full path to the scope
    fn full_path_string(&self) -> String {
        if self.path.strs().is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.path, self.name)
        }
    }

    fn full_path(&self) -> Vec<String> {
        self.path
            .strs()
            .iter()
            .cloned()
            .chain([self.name.clone()])
            .collect()
    }

    fn from_strs(s: &[&str]) -> Self {
        Self {
            path: ScopeRef::from_strs(&s[..(s.len() - 1)]),
            name: s
                .last()
                .expect("from_strs called with an empty string")
                .to_string(),
            id: VarId::default(),
        }
    }

    fn clear_id(&mut self) {
        self.id = VarId::default();
    }

    fn cxxrtl_repr(&self) -> String {
        self.full_path().join(" ")
    }
}

#[local_impl::local_impl]
impl FieldRefExt for FieldRef {
    fn without_fields(root: VariableRef) -> Self {
        Self {
            root,
            field: vec![],
        }
    }

    fn from_strs(root: &[&str], field: &[&str]) -> Self {
        Self {
            root: VariableRef::from_strs(root),
            field: field.iter().map(|s| s.to_string()).collect(),
        }
    }
}

pub enum WaveContainer {
    Wellen(Box<WellenContainer>),
    /// A wave container that contains nothing. Currently, the only practical use for this is
    /// a placehodler when serializing and deserializing wave state.
    Empty,
    #[cfg(not(target_arch = "wasm32"))]
    Cxxrtl(Mutex<CxxrtlContainer>),
}

impl WaveContainer {
    pub fn new_waveform(hierarchy: std::sync::Arc<wellen::Hierarchy>) -> Self {
        WaveContainer::Wellen(Box::new(WellenContainer::new(hierarchy, None)))
    }

    pub fn new_remote_waveform(
        server_url: String,
        hierarchy: std::sync::Arc<wellen::Hierarchy>,
    ) -> Self {
        WaveContainer::Wellen(Box::new(WellenContainer::new(hierarchy, Some(server_url))))
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
            WaveContainer::Cxxrtl(_) => vec![], // FIXME: List variable names
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
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().variables_in_module(scope),
        }
    }

    pub fn no_variables_in_scope(&self, scope: &ScopeRef) -> bool {
        match self {
            WaveContainer::Wellen(f) => f.no_variables_in_scope(scope),
            WaveContainer::Empty => true,
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().variables_in_module(scope).is_empty(),
        }
    }

    /// Loads multiple variables at once. This is useful when we want to add multiple variables in one go.
    pub fn load_variables<S: AsRef<VariableRef>, T: Iterator<Item = S>>(
        &mut self,
        variables: T,
    ) -> Result<Option<LoadSignalsCmd>> {
        match self {
            WaveContainer::Wellen(f) => f.load_variables(variables),
            WaveContainer::Empty => bail!("Cannot load variables from empty container."),
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => {
                c.get_mut().unwrap().load_variables(variables);
                Ok(None)
            }
        }
    }

    /// Callback for when wellen signals have been loaded. Might lead to a new load variable
    /// command since new variables might have been requested in the meantime
    pub fn on_signals_loaded(&mut self, res: LoadSignalsResult) -> Result<Option<LoadSignalsCmd>> {
        match self {
            WaveContainer::Wellen(f) => f.on_signals_loaded(res),
            WaveContainer::Empty => {
                bail!("on_load_signals should only be called with the wellen backend.")
            }
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(_) => {
                bail!("on_load_signals should only be called with the wellen backend.")
            }
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
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().variable_meta(r),
        }
    }

    /// Query the value of the variable at a certain time step.
    /// Returns `None` if we do not have any values for the variable.
    /// That generally happens if the corresponding variable is still being loaded.
    pub fn query_variable(
        &self,
        variable: &VariableRef,
        time: &BigUint,
    ) -> Result<Option<QueryResult>> {
        match self {
            WaveContainer::Wellen(f) => f.query_variable(variable, time),
            WaveContainer::Empty => bail!("Querying variable from empty wave container"),
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => Ok(c.lock().unwrap().query_variable(variable, time)),
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
                .map(|m| m.strs().last().cloned().unwrap_or("root".to_string()))
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
                .map(|t| t.as_femtoseconds()),
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

    /// If [`WaveContainer::simulation_status`] is `Some(SimulationStatus::Paused)`, attempt to unpause the
    /// simulation otherwise does nothing
    pub fn unpause_simulation(&self) {
        match self {
            WaveContainer::Wellen(_) => {}
            WaveContainer::Empty => {}
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().unpause(),
        }
    }

    /// See [`WaveContainer::unpause_simulation`]
    pub fn pause_simulation(&self) {
        match self {
            WaveContainer::Wellen(_) => {}
            WaveContainer::Empty => {}
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().pause(),
        }
    }

    /// Called for `wellen` container, when the body of the waveform file has been parsed.
    pub fn wellen_add_body(&mut self, body: BodyResult) -> Result<Option<LoadSignalsCmd>> {
        match self {
            WaveContainer::Wellen(inner) => inner.add_body(body),
            _ => {
                bail!("Should never call this function on a non wellen container!")
            }
        }
    }

    pub fn body_loaded(&self) -> bool {
        match self {
            WaveContainer::Wellen(inner) => inner.body_loaded(),
            WaveContainer::Empty => true,
            #[cfg(not(target_arch = "wasm32"))]
            WaveContainer::Cxxrtl(_) => true,
        }
    }
}
