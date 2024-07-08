use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct ScopeRef<ScopeId> {
    pub strs: Vec<String>,
    /// Backend specific numeric ID. Performance optimization.
    #[serde(
        skip,
        default = "ScopeId::default",
        bound(deserialize = "ScopeId: Default")
    )]
    pub id: ScopeId,
}

impl<ScopeId> AsRef<ScopeRef<ScopeId>> for ScopeRef<ScopeId> {
    fn as_ref(&self) -> &ScopeRef<ScopeId> {
        self
    }
}

impl<ScopeId> Hash for ScopeRef<ScopeId> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // id is intentionally not hashed, since it is only a performance hint
        self.strs.hash(state)
    }
}

impl<ScopeId> PartialEq for ScopeRef<ScopeId> {
    fn eq(&self, other: &Self) -> bool {
        // id is intentionally not compared, since it is only a performance hint
        self.strs.eq(&other.strs)
    }
}

impl<ScopeId> Display for ScopeRef<ScopeId> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.strs.join("."))
    }
}
