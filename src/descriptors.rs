use fastwave_backend::{ScopeIdx, SignalIdx};

use crate::VcdData;

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
