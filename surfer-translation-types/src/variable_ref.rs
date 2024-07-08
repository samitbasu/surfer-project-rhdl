use crate::ScopeRef;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

// FIXME: We'll be cloning these quite a bit, I wonder if a `Cow<&str>` or Rc/Arc would be better
#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct VariableRef<VarId, ScopeId> {
    /// Path in the scope hierarchy to where this variable resides
    #[serde(bound(deserialize = "ScopeId: Default", serialize = ""))]
    pub path: ScopeRef<ScopeId>,
    /// Name of the variable in its hierarchy
    pub name: String,
    /// Backend specific numeric ID. Performance optimization.
    #[serde(
        skip,
        default = "VarId::default",
        bound(deserialize = "VarId: Default")
    )]
    pub id: VarId,
}

impl<VarId, ScopeId> AsRef<VariableRef<VarId, ScopeId>> for VariableRef<VarId, ScopeId> {
    fn as_ref(&self) -> &VariableRef<VarId, ScopeId> {
        self
    }
}

impl<VarId, ScopeId> Hash for VariableRef<VarId, ScopeId> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // id is intentionally not hashed, since it is only a performance hint
        self.path.hash(state);
        self.name.hash(state);
    }
}

impl<VarId, ScopeId> PartialEq for VariableRef<VarId, ScopeId> {
    fn eq(&self, other: &Self) -> bool {
        // id is intentionally not compared, since it is only a performance hint
        self.path.eq(&other.path) && self.name.eq(&other.name)
    }
}
