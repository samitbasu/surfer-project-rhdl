use crate::ScopeRef;
use extism_convert::{FromBytes, Json, ToBytes};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

// FIXME: We'll be cloning these quite a bit, I wonder if a `Cow<&str>` or Rc/Arc would be better
#[derive(Clone, Debug, Eq, Serialize, Deserialize, ToBytes, FromBytes)]
#[encoding(Json)]
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

impl<VarId, ScopeId> VariableRef<VarId, ScopeId> {
    pub fn map_ids<VarId2, ScopeId2>(
        self,
        mut var_fn: impl FnMut(VarId) -> VarId2,
        scope_fn: impl FnMut(ScopeId) -> ScopeId2,
    ) -> VariableRef<VarId2, ScopeId2> {
        VariableRef {
            path: self.path.map_id(scope_fn),
            name: self.name,
            id: var_fn(self.id),
        }
    }

    pub fn full_path(&self) -> Vec<String> {
        self.path
            .strs
            .iter()
            .cloned()
            .chain([self.name.clone()])
            .collect()
    }
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
