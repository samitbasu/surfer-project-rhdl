use fastwave_backend::{ScopeIdx, SignalIdx};

use crate::{view::TraceIdx, VcdData};

#[derive(Debug)]
pub struct PathDescriptor(pub SignalDescriptor, pub Vec<String>);

impl PathDescriptor {
    pub fn from_traceidx(t: &TraceIdx) -> Self {
        PathDescriptor(SignalDescriptor::Id(t.0), t.1.clone())
    }

    pub fn from_named(name: String, path: Vec<String>) -> Self {
        Self(SignalDescriptor::Name(name), path)
    }
}

#[derive(Debug)]
pub enum SignalDescriptor {
    Id(SignalIdx),
    Name(String),
}

impl SignalDescriptor {
    pub fn resolve(&self, vcd: &VcdData) -> Option<SignalIdx> {
        match self {
            SignalDescriptor::Id(id) => Some(*id),
            SignalDescriptor::Name(name) => vcd.signals_to_ids.get(name).copied(),
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
