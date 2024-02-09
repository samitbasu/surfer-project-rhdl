use chrono::prelude::{DateTime, Utc};
use color_eyre::{eyre::bail, Result};
use num::BigUint;
use serde::{Deserialize, Serialize};
use wellen::{ScopeRef, VarRef, Waveform};

use crate::wellen::var_to_meta;
use crate::{
    signal_type::SignalType,
    time::{TimeScale, TimeUnit},
    wellen::WellenContainer,
};

#[derive(Debug, PartialEq)]
pub enum SignalValue {
    BigUint(BigUint),
    String(String),
}

pub struct MetaData {
    pub date: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub timescale: TimeScale,
}
#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct ModuleRef {
    strs: Vec<String>,
    /// Backend specific numeric ID. Performance optimization.
    #[serde(skip, default = "__wave_container_scope_id_none")]
    id: WaveContainerScopeId,
}

impl std::hash::Hash for ModuleRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // id is intentionally not hashed, since it is only a performance hint
        self.strs.hash(state)
    }
}

impl std::cmp::PartialEq for ModuleRef {
    fn eq(&self, other: &Self) -> bool {
        // id is intentionally not compared, since it is only a performance hint
        self.strs.eq(&other.strs)
    }
}

/// A backend-specific, numeric reference for fast access to the associated scope.
#[derive(Clone, Debug, PartialEq, Eq)]
enum WaveContainerScopeId {
    None,
    Wellen(wellen::ScopeRef),
}

fn __wave_container_scope_id_none() -> WaveContainerScopeId {
    WaveContainerScopeId::None
}

impl ModuleRef {
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

    pub fn from_strs_with_wellen_id<S: ToString>(s: &[S], id: ScopeRef) -> Self {
        let mut a = Self::from_strs(s);
        a.id = WaveContainerScopeId::Wellen(id);
        a
    }

    /// Creates a ModuleRef from a string with each module separated by `.`
    pub fn from_hierarchy_string(s: &str) -> Self {
        let strs = s.split('.').map(|x| x.to_string()).collect();
        let id = WaveContainerScopeId::None;
        Self { strs, id }
    }

    pub fn with_submodule(&self, submodule: String) -> Self {
        let mut result = self.clone();
        result.strs.push(submodule);
        // the result refers to a different module, which we do not know the ID of
        result.id = WaveContainerScopeId::None;
        result
    }

    pub(crate) fn name(&self) -> String {
        self.strs.last().cloned().unwrap_or_default()
    }

    pub(crate) fn strs(&self) -> &[String] {
        &self.strs
    }

    pub(crate) fn get_wellen_id(&self) -> Option<ScopeRef> {
        match self.id {
            WaveContainerScopeId::Wellen(id) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn with_wellen_id(&self, id: ScopeRef) -> Self {
        let mut out = self.clone();
        out.id = WaveContainerScopeId::Wellen(id);
        out
    }
}

impl std::fmt::Display for ModuleRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.strs.join("."))
    }
}

// FIXME: We'll be cloning these quite a bit, I wonder if a `Cow<&str>` or Rc/Arc would be better
#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct SignalRef {
    /// Path in the module hierarchy to where this signal resides
    pub path: ModuleRef,
    /// Name of the signal in its hierarchy
    pub name: String,
    /// Backend specific numeric ID. Performance optimization.
    #[serde(skip, default = "__wave_container_var_id_none")]
    id: WaveContainerVarId,
}

impl AsRef<SignalRef> for SignalRef {
    fn as_ref(&self) -> &SignalRef {
        self
    }
}

impl std::hash::Hash for SignalRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // id is intentionally not hashed, since it is only a performance hint
        self.path.hash(state);
        self.name.hash(state);
    }
}

impl std::cmp::PartialEq for SignalRef {
    fn eq(&self, other: &Self) -> bool {
        // id is intentionally not compared, since it is only a performance hint
        self.path.eq(&other.path) && self.name.eq(&other.name)
    }
}

/// A backend-specific, numeric reference for fast access to the associated variable.
#[derive(Clone, Debug, PartialEq, Eq)]
enum WaveContainerVarId {
    None,
    Wellen(wellen::VarRef),
}

fn __wave_container_var_id_none() -> WaveContainerVarId {
    WaveContainerVarId::None
}

impl SignalRef {
    pub fn new(path: ModuleRef, name: String) -> Self {
        Self {
            path,
            name,
            id: WaveContainerVarId::None,
        }
    }

    pub(crate) fn new_with_wave_id(path: ModuleRef, name: String, id: VarRef) -> Self {
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
                path: ModuleRef::empty(),
                name: String::new(),
                id: WaveContainerVarId::None,
            }
        } else {
            Self {
                path: ModuleRef::from_strs(&components[..(components.len()) - 1]),
                name: components.last().unwrap().to_string(),
                id: WaveContainerVarId::None,
            }
        }
    }

    /// A human readable full path to the module
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
            path: ModuleRef::from_strs(&s[..(s.len() - 1)]),
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

/// A reference to a field of a larger signal, such as a field in a struct. The fields
/// are the recursive path to the fields inside the (translated) root
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRef {
    pub root: SignalRef,
    pub field: Vec<String>,
}

impl FieldRef {
    pub fn without_fields(root: SignalRef) -> Self {
        Self {
            root,
            field: vec![],
        }
    }

    #[cfg(test)]
    pub fn from_strs(root: &[&str], field: &[&str]) -> Self {
        Self {
            root: SignalRef::from_strs(root),
            field: field.into_iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Debug)]
pub struct QueryResult {
    pub current: Option<(BigUint, SignalValue)>,
    pub next: Option<BigUint>,
}

#[derive(Debug)]
pub enum WaveContainer {
    Wellen(WellenContainer),
    /// A wave container that contains nothing. Currently, the only practical use for this is
    /// a placehodler when serializing and deserializing wave state.
    Empty,
}

impl WaveContainer {
    pub fn new_waveform(waveform: Waveform) -> Self {
        WaveContainer::Wellen(WellenContainer::new(waveform))
    }

    /// Creates a new empty wave container. Should only be used as a default for serde. If
    /// no wave container is present, the WaveData should be None, rather than this being
    /// Empty
    pub fn __new_empty() -> Self {
        WaveContainer::Empty
    }

    /// Returns the full names of all signals (i.e. variables) in the design.
    pub fn signal_names(&self) -> Vec<String> {
        match self {
            WaveContainer::Wellen(f) => f.signal_names(),
            WaveContainer::Empty => vec![],
        }
    }

    pub fn signals_in_module(&self, module: &ModuleRef) -> Vec<SignalRef> {
        match self {
            WaveContainer::Wellen(f) => f.signals_in_module(module),
            WaveContainer::Empty => vec![],
        }
    }

    /// Loads a signal into memory. Needs to be called before using `query_signal` on the signal.
    pub fn load_signal<'a>(&mut self, r: &'a SignalRef) -> Result<SignalMeta<'a>> {
        match self {
            WaveContainer::Wellen(f) => f.load_signal(r).map(|v| var_to_meta(v, r)),
            WaveContainer::Empty => bail!("Cannot load signal from empty container."),
        }
    }

    /// Loads multiple signals at once. This is useful when we want to add multiple signals in one go.
    pub fn load_signals<S: AsRef<SignalRef>, T: Iterator<Item = S>>(
        &mut self,
        signals: T,
    ) -> Result<()> {
        match self {
            WaveContainer::Wellen(f) => f.load_signals(signals),
            WaveContainer::Empty => bail!("Cannot load signal from empty container."),
        }
    }

    pub fn signal_meta<'a>(&'a self, r: &'a SignalRef) -> Result<SignalMeta> {
        match self {
            WaveContainer::Wellen(f) => {
                let var = f.get_var(r)?;
                Ok(var_to_meta(var, r))
            }
            WaveContainer::Empty => bail!("Getting meta from empty wave container"),
        }
    }

    pub fn query_signal(&self, signal: &SignalRef, time: &BigUint) -> Result<QueryResult> {
        match self {
            WaveContainer::Wellen(f) => f.query_signal(signal, time),
            WaveContainer::Empty => bail!("Querying signal from empty wave container"),
        }
    }

    /// Looks up the signal _by name_ and returns a new reference with an updated `id` if the signal is found.
    pub fn update_signal_ref(&self, signal: &SignalRef) -> Option<SignalRef> {
        match self {
            WaveContainer::Wellen(f) => f.update_signal_ref(signal),
            WaveContainer::Empty => None,
        }
    }

    /// Returns the full names of all modules (i.e., scopes) in the design.
    pub fn module_names(&self) -> Vec<String> {
        match self {
            WaveContainer::Wellen(f) => f.module_names(),
            WaveContainer::Empty => vec![],
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
        }
    }

    pub fn root_modules(&self) -> Vec<ModuleRef> {
        match self {
            WaveContainer::Wellen(f) => f.root_modules(),
            WaveContainer::Empty => vec![],
        }
    }

    pub fn child_modules(&self, module: &ModuleRef) -> Result<Vec<ModuleRef>> {
        match self {
            WaveContainer::Wellen(f) => f.child_modules(module),
            WaveContainer::Empty => bail!("Getting child modules from empty wave container"),
        }
    }

    pub fn max_timestamp(&self) -> Option<BigUint> {
        match self {
            WaveContainer::Wellen(f) => f.max_timestamp(),
            WaveContainer::Empty => None,
        }
    }

    pub fn module_exists(&self, module: &ModuleRef) -> bool {
        match self {
            WaveContainer::Wellen(f) => f.module_exists(module),
            WaveContainer::Empty => false,
        }
    }

    /// Returns a human readable string with information about a scope.
    /// The module name itself should not be included, since it will be prepended automatically.
    pub fn get_scope_tooltip_data(&self, scope: &ModuleRef) -> String {
        match self {
            WaveContainer::Wellen(f) => f.get_scope_tooltip_data(scope),
            WaveContainer::Empty => "".to_string(),
        }
    }
}

pub struct SignalMeta<'a> {
    pub sig: &'a SignalRef,
    pub num_bits: Option<u32>,
    pub signal_type: Option<SignalType>,
    pub index: Option<String>,
}
