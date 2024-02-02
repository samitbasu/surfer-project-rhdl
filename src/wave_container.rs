use chrono::prelude::{DateTime, Utc};
use color_eyre::{
    eyre::{bail, Context},
    Result,
};
use num::BigUint;
use serde::{Deserialize, Serialize};

use crate::{
    fast_wave_container::FastWaveContainer,
    signal_type::SignalType,
    time::{TimeScale, TimeUnit},
};

#[derive(Debug, PartialEq)]
pub enum SignalValue {
    BigUint(BigUint),
    String(String),
}

impl From<fastwave_backend::SignalValue> for SignalValue {
    fn from(val: fastwave_backend::SignalValue) -> Self {
        match val {
            fastwave_backend::SignalValue::BigUint(v) => SignalValue::BigUint(v),
            fastwave_backend::SignalValue::String(s) => SignalValue::String(s),
        }
    }
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
pub struct SignalRef {
    /// Path in the module hierarchy to where this signal resides
    pub path: ModuleRef,
    /// Name of the signal in its hierarchy
    pub name: String,
}

impl SignalRef {
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

/// A reference to a field of a larger signal, such as a field in a struct. The fields
/// are the recursive path to the fields inside the (translated) root
#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
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

pub struct QueryResult {
    pub current: Option<(BigUint, SignalValue)>,
    pub next: Option<BigUint>,
}

#[derive(Debug)]
pub enum WaveContainer {
    Fwb(FastWaveContainer),
    /// A wave container that contains nothing. Currently, the only practical use for this is
    /// a placehodler when serializing and deserializing wave state.
    Empty,
}

impl WaveContainer {
    pub fn new_vcd(vcd: fastwave_backend::VCD) -> Self {
        WaveContainer::Fwb(FastWaveContainer::new(vcd))
    }

    /// Creates a new empty wave container. Should only be used as a default for serde. If
    /// no wave container is present, the WaveData should be None, rather than this being
    /// Empty
    pub fn __new_empty() -> Self {
        WaveContainer::Empty
    }

    pub fn signals(&self) -> &[SignalRef] {
        match self {
            WaveContainer::Fwb(f) => f.signals(),
            WaveContainer::Empty => &[],
        }
    }

    pub fn signals_in_module(&self, module: &ModuleRef) -> Vec<SignalRef> {
        match self {
            WaveContainer::Fwb(f) => f.signals_in_module(module),
            WaveContainer::Empty => vec![],
        }
    }

    pub fn signal_meta<'a>(&'a self, r: &'a SignalRef) -> Result<SignalMeta> {
        match self {
            WaveContainer::Fwb(f) => {
                f.fwb_signal(r)
                    .context("When getting signal metadata")
                    .map(|signal| SignalMeta {
                        sig: r,
                        num_bits: signal.num_bits(),
                        signal_type: signal.signal_type().cloned().map(SignalType::from),
                        index: signal.index(),
                    })
            }
            WaveContainer::Empty => bail!("Getting meta from empty wave container"),
        }
    }

    pub fn query_signal(&self, signal: &SignalRef, time: &BigUint) -> Result<QueryResult> {
        match self {
            WaveContainer::Fwb(f) => f.query_signal(signal, time),
            WaveContainer::Empty => bail!("Querying signal from empty wave container"),
        }
    }

    pub fn signal_exists(&self, signal: &SignalRef) -> bool {
        match self {
            WaveContainer::Fwb(f) => f.signal_exists(signal),
            WaveContainer::Empty => false,
        }
    }

    pub fn modules(&self) -> Vec<ModuleRef> {
        match self {
            WaveContainer::Fwb(f) => f.modules(),
            WaveContainer::Empty => vec![],
        }
    }

    pub fn has_module(&self, module: &ModuleRef) -> bool {
        match self {
            WaveContainer::Fwb(f) => f.module_map.contains_key(module),
            WaveContainer::Empty => false,
        }
    }

    pub fn metadata(&self) -> MetaData {
        match self {
            WaveContainer::Fwb(f) => MetaData {
                date: f.inner.metadata.date,
                version: f.inner.metadata.version.as_ref().map(|m| m.0.clone()),
                timescale: TimeScale {
                    unit: TimeUnit::from(f.inner.metadata.timescale.1),
                    multiplier: f.inner.metadata.timescale.0,
                },
            },
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
            WaveContainer::Fwb(f) => f.root_modules(),
            WaveContainer::Empty => vec![],
        }
    }

    pub fn child_modules(&self, module: &ModuleRef) -> Result<Vec<ModuleRef>> {
        match self {
            WaveContainer::Fwb(f) => f.child_modules(module),
            WaveContainer::Empty => bail!("Getting child modules from empty wave container"),
        }
    }

    pub fn max_timestamp(&self) -> Option<BigUint> {
        match self {
            WaveContainer::Fwb(f) => f.inner.max_timestamp().clone(),
            WaveContainer::Empty => None,
        }
    }

    pub fn module_exists(&self, module: &ModuleRef) -> bool {
        match self {
            WaveContainer::Fwb(f) => f.module_exists(module),
            WaveContainer::Empty => false,
        }
    }
}

pub struct SignalMeta<'a> {
    pub sig: &'a SignalRef,
    pub num_bits: Option<u32>,
    pub signal_type: Option<SignalType>,
    pub index: Option<String>,
}
