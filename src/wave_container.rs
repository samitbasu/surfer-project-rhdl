use chrono::prelude::{DateTime, Utc};
use color_eyre::{eyre::bail, Result};
use num::BigUint;
use serde::{Deserialize, Serialize};
use wellen::Waveform;

use crate::wellen::var_to_meta;
use crate::{
    signal_type::SignalType,
    time::{TimeScale, TimeUnit},
    wellen::WellenContainer,
};

#[derive(Debug, PartialEq)]
pub enum VariableValue {
    BigUint(BigUint),
    String(String),
}

pub struct MetaData {
    pub date: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub timescale: TimeScale,
}
#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ModuleRef(pub Vec<String>);

impl ModuleRef {
    pub fn from_strs(s: &[&str]) -> Self {
        Self(s.iter().map(|s| s.to_string()).collect())
    }

    /// Creates a ModuleRef from a string with each module separated by `.`
    pub fn from_hierarchy_string(s: &str) -> Self {
        Self(s.split('.').map(|x| x.to_string()).collect())
    }

    pub fn with_submodule(&self, submodule: String) -> Self {
        let mut result = self.clone();
        result.0.push(submodule);
        result
    }

    pub(crate) fn name(&self) -> String {
        self.0.last().cloned().unwrap_or_default()
    }
}

impl std::fmt::Display for ModuleRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0.join("."))
    }
}

// FIXME: We'll be cloning these quite a bit, I wonder if a `Cow<&str>` or Rc/Arc would be better
#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct VariableRef {
    /// Path in the module hierarchy to where this variable resides
    pub path: ModuleRef,
    /// Name of the variable in its hierarchy
    pub name: String,
}

impl AsRef<VariableRef> for VariableRef {
    fn as_ref(&self) -> &VariableRef {
        self
    }
}

impl VariableRef {
    pub fn new(path: ModuleRef, name: String) -> Self {
        Self { path, name }
    }

    pub fn from_hierarchy_string(s: &str) -> Self {
        let components = s.split('.').map(|s| s.to_string()).collect::<Vec<_>>();

        if components.is_empty() {
            Self {
                path: ModuleRef(vec![]),
                name: String::new(),
            }
        } else {
            Self {
                path: ModuleRef(components[..(components.len()) - 1].to_vec()),
                name: components.last().unwrap().to_string(),
            }
        }
    }

    /// A human readable full path to the module
    pub fn full_path_string(&self) -> String {
        if self.path.0.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.path, self.name)
        }
    }

    pub fn full_path(&self) -> Vec<String> {
        self.path
            .0
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
        }
    }
}

/// A reference to a field of a larger variable, such as a field in a struct. The fields
/// are the recursive path to the fields inside the (translated) root
#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
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

#[derive(Debug)]
pub struct QueryResult {
    pub current: Option<(BigUint, VariableValue)>,
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

    pub fn variables(&self) -> Vec<VariableRef> {
        match self {
            WaveContainer::Wellen(f) => f.variables(),
            WaveContainer::Empty => vec![],
        }
    }

    pub fn variables_in_scope(&self, scope: &ModuleRef) -> Vec<VariableRef> {
        match self {
            WaveContainer::Wellen(f) => f.variables_in_scope(scope),
            WaveContainer::Empty => vec![],
        }
    }

    /// Loads a variable into memory. Needs to be called before using `query_variable` on the variable.
    pub fn load_variable<'a>(&mut self, r: &'a VariableRef) -> Result<VariableMeta<'a>> {
        match self {
            WaveContainer::Wellen(f) => f.load_variable(r).map(|v| var_to_meta(v, r)),
            WaveContainer::Empty => bail!("Cannot load variable from empty container."),
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
        }
    }

    pub fn variable_meta<'a>(&'a self, r: &'a VariableRef) -> Result<VariableMeta> {
        match self {
            WaveContainer::Wellen(f) => {
                let var = f.get_var(r)?;
                Ok(var_to_meta(var, r))
            }
            WaveContainer::Empty => bail!("Getting meta from empty wave container"),
        }
    }

    pub fn query_variable(&self, variable: &VariableRef, time: &BigUint) -> Result<QueryResult> {
        match self {
            WaveContainer::Wellen(f) => f.query_variable(variable, time),
            WaveContainer::Empty => bail!("Querying variable from empty wave container"),
        }
    }

    pub fn variable_exists(&self, variable: &VariableRef) -> bool {
        match self {
            WaveContainer::Wellen(f) => f.variable_exists(variable),
            WaveContainer::Empty => false,
        }
    }

    pub fn modules(&self) -> Vec<ModuleRef> {
        match self {
            WaveContainer::Wellen(f) => f.modules(),
            WaveContainer::Empty => vec![],
        }
    }

    // FIXME: this seems to alias the module_exists function. Remove?
    pub fn has_module(&self, module: &ModuleRef) -> bool {
        match self {
            WaveContainer::Wellen(f) => f.module_exists(module),
            WaveContainer::Empty => false,
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
}

pub struct VariableMeta<'a> {
    pub sig: &'a VariableRef,
    pub num_bits: Option<u32>,
    pub signal_type: Option<SignalType>,
    pub index: Option<String>,
}
