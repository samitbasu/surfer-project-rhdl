use crate::{VariableMeta, VariableValue};
use extism_convert::{FromBytes, Json, ToBytes};
use serde::{Deserialize, Serialize};

#[derive(FromBytes, ToBytes, Deserialize, Serialize)]
#[encoding(Json)]
pub struct TranslateParams {
    pub variable: VariableMeta<(), ()>,
    pub value: VariableValue,
}
