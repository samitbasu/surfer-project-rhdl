use serde::Serialize;

use super::command::CxxrtlCommand;

#[derive(Serialize, Debug)]
#[serde(tag = "type")]
#[allow(non_camel_case_types)]
pub(crate) enum CSMessage {
    greeting { version: i64 },
    command(CxxrtlCommand),
}
