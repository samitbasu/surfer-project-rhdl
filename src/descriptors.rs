use fastwave_backend::{ScopeIdx, SignalIdx};

use crate::{view::TraceIdx, VcdData};

/// A signal name which may contain an index surrounded by `[]`.
#[derive(Debug)]
pub struct NameWithIndex(pub String, pub Option<String>);

impl NameWithIndex {
    pub fn from_full_string(s: String) -> Self {
        let parts = s.split('[').collect::<Vec<_>>();

        match parts.as_slice() {
            [] => Self(String::new(), None),
            [name] => Self(name.trim().to_string(), None),
            [name, index, ..] => Self(name.to_string(), Some(format!("[{index}"))),
        }
    }
}

#[derive(Debug)]
pub struct PathDescriptor(pub SignalDescriptor, pub Vec<String>);

impl PathDescriptor {
    pub fn from_traceidx(t: &TraceIdx) -> Self {
        PathDescriptor(SignalDescriptor::Id(t.0), t.1.clone())
    }

    pub fn from_named(name: String, idx: Option<String>, path: Vec<String>) -> Self {
        Self(SignalDescriptor::Name(NameWithIndex(name, idx)), path)
    }
}

#[derive(Debug)]
pub enum SignalDescriptor {
    Id(SignalIdx),
    Name(NameWithIndex),
}

impl SignalDescriptor {
    pub fn resolve(&self, vcd: &VcdData) -> Option<SignalIdx> {
        match self {
            SignalDescriptor::Id(id) => Some(*id),
            SignalDescriptor::Name(name) => vcd
                .signals_to_ids
                .get(&(name.0.clone(), name.1.clone()))
                .copied(),
        }
    }
}

impl From<SignalIdx> for SignalDescriptor {
    fn from(value: SignalIdx) -> Self {
        Self::Id(value)
    }
}

#[derive(Debug)]
pub enum ScopeDescriptor {
    Id(ScopeIdx),
    Name(String),
}

impl ScopeDescriptor {
    pub fn resolve(&self, vcd: &VcdData) -> Option<ScopeIdx> {
        match self {
            ScopeDescriptor::Id(id) => Some(*id),
            ScopeDescriptor::Name(name) => vcd.scopes_to_ids.get(name).copied(),
        }
    }
}

impl From<ScopeIdx> for ScopeDescriptor {
    fn from(value: ScopeIdx) -> Self {
        Self::Id(value)
    }
}
