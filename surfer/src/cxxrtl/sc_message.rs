use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cxxrtl_container::{CxxrtlItem, CxxrtlScope};

use super::timestamp::CxxrtlTimestamp;

#[derive(Deserialize, Serialize, Debug)]
pub struct CxxrtlSample {
    pub time: CxxrtlTimestamp,
    pub item_values: String,
}

#[derive(Deserialize, Debug)]
pub(crate) struct Features {}

#[derive(Deserialize, Debug, Clone)]
#[allow(non_camel_case_types)]
pub enum SimulationStatusType {
    running,
    paused,
    finished,
}
#[derive(Deserialize, Debug, Clone)]
pub struct CxxrtlSimulationStatus {
    pub status: SimulationStatusType,
    pub latest_time: CxxrtlTimestamp,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "command")]
#[allow(non_camel_case_types)]
pub(crate) enum CommandResponse {
    list_scopes {
        scopes: HashMap<String, CxxrtlScope>,
    },
    list_items {
        items: HashMap<String, CxxrtlItem>,
    },
    get_simulation_status(CxxrtlSimulationStatus),
    query_interval {
        samples: Vec<CxxrtlSample>,
    },
    reference_items,
    run_simulation,
    pause_simulation {
        time: CxxrtlTimestamp,
    },
}

#[derive(Deserialize, Debug, Clone)]
#[allow(non_camel_case_types)]
pub(crate) enum PauseCause {
    until_time,
    until_diagnostics,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "event")]
#[allow(non_camel_case_types)]
pub(crate) enum Event {
    simulation_paused {
        time: CxxrtlTimestamp,
        #[allow(unused)]
        cause: PauseCause,
    },
    simulation_finished {
        time: CxxrtlTimestamp,
    },
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[allow(non_camel_case_types, unused)]
pub(crate) enum SCMessage {
    greeting {
        version: i64,
        commands: Vec<String>,
        events: Vec<String>,
        features: Features,
    },
    response(CommandResponse),
    event(Event),
}
