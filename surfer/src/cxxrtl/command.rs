use serde::Serialize;

use super::timestamp::CxxrtlTimestamp;

#[derive(Serialize, Debug)]
#[allow(non_camel_case_types, unused)]
pub(crate) enum Diagnostic {
    assert,
    assume,
    print,
}

#[derive(Serialize, Debug)]
#[serde(tag = "command")]
#[allow(non_camel_case_types)]
pub(crate) enum CxxrtlCommand {
    list_scopes,
    list_items {
        scope: Option<String>,
    },
    get_simulation_status,
    query_interval {
        interval: (CxxrtlTimestamp, CxxrtlTimestamp),
        collapse: bool,
        items: Option<String>,
        item_values_encoding: &'static str,
        diagnostics: bool,
    },
    reference_items {
        reference: String,
        items: Vec<Vec<String>>,
    },
    run_simulation {
        until_time: Option<CxxrtlTimestamp>,
        until_diagnostics: Vec<Diagnostic>,
        sample_item_values: bool,
    },
    pause_simulation,
}
