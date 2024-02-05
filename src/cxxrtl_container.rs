use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    io::{Read, Write},
    net::TcpStream,
    str::FromStr,
    time::Duration,
};

use base64::prelude::*;
use color_eyre::{
    eyre::{bail, Context},
    Result,
};
use itertools::Itertools;
use log::{error, info, trace, warn};
use num::BigUint;
use serde::{Deserialize, Deserializer, Serialize};
use spade_common::num_ext::InfallibleToBigUint;

use crate::{
    cxxrtl::timestamp::CxxrtlTimestamp,
    wave_container::{ModuleRef, QueryResult, SignalMeta, SignalRef, SignalValue},
};

impl ModuleRef {
    fn cxxrtl_repr(&self) -> String {
        self.0.iter().join(" ")
    }
}

impl SignalRef {
    fn cxxrtl_repr(&self) -> String {
        self.full_path().join(" ")
    }
}

#[derive(Serialize, Debug)]
#[serde(tag = "command")]
#[allow(non_camel_case_types)]
enum CxxrtlCommand {
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
        daignostics: bool,
    },
}

#[derive(Serialize, Debug)]
#[serde(tag = "type")]
#[allow(non_camel_case_types)]
enum CSMessage {
    greeting { version: i64 },
    command(CxxrtlCommand),
}

#[derive(Deserialize, Debug)]
struct Features {
    item_values_encoding: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct CxxrtlScope {
    src: Option<String>,
    // TODO: More stuff
}

#[derive(Deserialize, Debug)]
struct CxxrtlItem {
    src: Option<String>, // TODO: More stuff
    #[serde(rename = "type")]
    ty: Option<String>,
    width: Option<u32>,
    lsb_at: Option<u32>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "command")]
#[allow(non_camel_case_types)]
enum CommandResponse {
    list_scopes {
        scopes: HashMap<String, CxxrtlScope>,
    },
    list_items {
        items: HashMap<String, CxxrtlItem>,
    },
    get_simulation_status {
        status: String,
        #[serde(deserialize_with = "CxxrtlTimestamp::deserialize")]
        latest_time: CxxrtlTimestamp,
    },
    query_interval {
        samples: Vec<CxxrtlSample>,
    },
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[allow(non_camel_case_types, unused)]
enum SCMessage {
    greeting {
        version: i64,
        commands: Vec<String>,
        events: Vec<String>,
        features: Features,
    },
    response(CommandResponse),
}

#[derive(Deserialize, Serialize, Debug)]
pub struct CxxrtlSample {
    time: CxxrtlTimestamp,
    item_values: String,
}

impl CxxrtlSample {
    fn into_signal_value(&self) -> Result<SignalValue> {
        Ok(SignalValue::String(
            String::from_utf8_lossy(&BASE64_STANDARD.decode(&self.item_values)?).to_string(),
        ))
    }
}

pub struct CxxrtlContainer {
    stream: TcpStream,
    read_buf: VecDeque<u8>,

    scopes_cache: Option<HashMap<ModuleRef, CxxrtlScope>>,
    module_item_cache: HashMap<ModuleRef, HashMap<SignalRef, CxxrtlItem>>,
    all_items_cache: HashMap<SignalRef, CxxrtlItem>,

    signal_values_cache: HashMap<SignalRef, BTreeMap<BigUint, CxxrtlSample>>,
}

macro_rules! send_command {
    ($self:expr, $command:expr, $response:pat => $on_response:expr) => {{
        $self.send_message(CSMessage::command($command))?;

        let response = $self
            .read_one_message()
            .context("failed to read scope response")?;

        if let SCMessage::response($response) = response {
            $on_response
        } else {
            bail!(
                "Did not get a {} response from cxxrtl. Got {response:?} instead",
                stringify! {$response}
            )
        }
    }};
}

impl CxxrtlContainer {
    pub fn new(addr: &str) -> Result<Self> {
        let mut stream =
            TcpStream::connect(addr).with_context(|| format!("Failed to connect to {addr}"))?;

        let greeting = serde_json::to_string(&CSMessage::greeting { version: 0 })
            .with_context(|| format!("Failed to encode greeting message"))?;
        stream.write_all(&greeting.as_bytes())?;
        stream.write_all(&[b'\0'])?;

        info!("Sending greeting to cxxrtl");
        trace!("C>S: {greeting}");

        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .with_context(|| "Failed to set stream timeout")?;

        let mut result = Self {
            stream,
            read_buf: VecDeque::new(),
            scopes_cache: None,
            module_item_cache: HashMap::new(),
            all_items_cache: HashMap::new(),
            signal_values_cache: HashMap::new(),
        };

        result
            .read_one_message()
            .context("Did not get a greeting back :(")?;

        info!("cxxrtl connected");

        Ok(result)
    }

    fn send_message(&mut self, message: CSMessage) -> Result<()> {
        self.stream.write_all(
            serde_json::to_string(&message)
                .with_context(|| format!("Failed to encode greeting message"))?
                .as_bytes(),
        )?;
        self.stream.write_all(&[b'\0'])?;

        trace!("cxxrtl: C>S: {message:?}");

        Ok(())
    }

    /// Reads bytes from the stream until we've found a message and return that message
    fn read_one_message(&mut self) -> Result<SCMessage> {
        while !self.read_buf.contains(&b'\0') {
            let mut buf = [0; 1024];
            let count = self.stream.read(&mut buf)?;

            if count != 0 {
                self.read_buf
                    .write_all(&buf[0..count])
                    .context("Failed to read from cxxrtl tcp socket")?;
            }
        }

        let idx = self
            .read_buf
            .iter()
            .enumerate()
            .find(|(_i, c)| **c == b'\0')
            .unwrap();
        let message = self.read_buf.drain(0..idx.0).collect::<Vec<_>>();
        // The newline should not be part of this or the next message message
        self.read_buf.pop_front();

        let decoded = serde_json::from_slice(&message).with_context(|| {
            format!(
                "Failed to decode message from cxxrtl. Message: '{}'",
                String::from_utf8_lossy(&message)
            )
        })?;

        trace!("cxxrtl: S>C: {decoded:?}");

        Ok(decoded)
    }

    fn fetch_scopes(&mut self) -> Result<HashMap<String, CxxrtlScope>> {
        send_command!(
            self,
            CxxrtlCommand::list_scopes,
            CommandResponse::list_scopes {scopes} => Ok(scopes)
        )
    }

    fn fetch_all_items(&mut self) -> Result<HashMap<String, CxxrtlItem>> {
        send_command!(
            self,
            CxxrtlCommand::list_items { scope: None },
            CommandResponse::list_items { items } => Ok(items)
        )
    }

    fn fetch_items_in_module(&mut self, module: &ModuleRef) -> Result<HashMap<String, CxxrtlItem>> {
        send_command!(self,
            CxxrtlCommand::list_items { scope: Some(module.cxxrtl_repr()), },
            CommandResponse::list_items{items} => Ok(items)
        )
    }

    fn item_list_to_hash_map(
        &self,
        items: HashMap<String, CxxrtlItem>,
    ) -> HashMap<SignalRef, CxxrtlItem> {
        items
            .into_iter()
            .filter_map(|(k, v)| {
                let sp = k.split(" ").collect::<Vec<_>>();

                if sp.is_empty() {
                    error!("Found an empty signal name and scope");
                    None
                } else {
                    Some((
                        SignalRef {
                            path: ModuleRef(
                                sp[0..sp.len() - 1]
                                    .into_iter()
                                    .map(|s| s.to_string())
                                    .collect(),
                            ),
                            name: sp.last().unwrap().to_string(),
                        },
                        v,
                    ))
                }
            })
            .collect()
    }

    fn scopes(&mut self) -> Option<&HashMap<ModuleRef, CxxrtlScope>> {
        if self.scopes_cache.is_none() {
            self.scopes_cache = self
                .fetch_scopes()
                .map_err(|e| error!("Failed to get modules: {e:#?}"))
                .ok()
                .map(|scopes| {
                    scopes
                        .into_iter()
                        .map(|(name, s)| {
                            (
                                ModuleRef(name.split(" ").map(|s| s.to_string()).collect()),
                                s,
                            )
                        })
                        .collect()
                });
        }

        self.scopes_cache.as_ref()
    }

    pub fn modules(&mut self) -> Vec<ModuleRef> {
        if let Some(scopes) = &self.scopes() {
            scopes.iter().map(|(k, _)| k.clone()).collect()
        } else {
            vec![]
        }
    }

    pub fn root_modules(&mut self) -> Vec<ModuleRef> {
        // In the CXXRtl protocol, the root scope is always ""
        if let Some(_) = &self.scopes() {
            vec![ModuleRef(vec![])]
        } else {
            vec![]
        }
    }

    pub fn module_exists(&mut self, module: &ModuleRef) -> bool {
        self.scopes()
            .map(|s| s.contains_key(module))
            .unwrap_or(false)
    }

    pub fn child_modules(&mut self, parent: &ModuleRef) -> Vec<ModuleRef> {
        self.scopes()
            .map(|scopes| {
                scopes
                    .keys()
                    .filter_map(|scope| {
                        if scope.0.len() == parent.0.len() + 1 {
                            if scope.0[0..parent.0.len()] == parent.0[0..parent.0.len()] {
                                Some(scope.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn signals_in_module(&mut self, module: &ModuleRef) -> Vec<SignalRef> {
        if !self.module_item_cache.contains_key(module) {
            if let Some(items) = self
                .fetch_items_in_module(module)
                .map_err(|e| info!("Failed to get items {e:#?}"))
                .ok()
            {
                self.module_item_cache
                    .insert(module.clone(), self.item_list_to_hash_map(items));
            }
        }

        self.module_item_cache
            .get(module)
            .map(|items| items.iter().map(|(k, _)| k.clone()).collect())
            .unwrap_or_default()
    }

    pub fn signal_meta(&mut self, signal: &SignalRef) -> Result<SignalMeta> {
        if !self.all_items_cache.contains_key(signal) {
            if let Some(items) = self
                .fetch_all_items()
                .map_err(|e| info!("Failed to get items {e:#?}"))
                .ok()
            {
                self.all_items_cache = self.item_list_to_hash_map(items)
            }
        }

        if let Some(cxxitem) = self.all_items_cache.get(signal) {
            Ok(SignalMeta {
                sig: signal.clone(),
                num_bits: cxxitem.width,
                // FIXME: Use the type that cxxrtl reports
                signal_type: None,
            })
        } else {
            bail!("Found no signal {signal:?}")
        }
    }

    pub fn max_timestamp(&mut self) -> Result<CxxrtlTimestamp> {
        send_command!(
            self,
            CxxrtlCommand::get_simulation_status,
            CommandResponse::get_simulation_status { status: _, latest_time: time } => {
                Ok(time)
            }
        )
    }

    fn query_signal_values(
        &mut self,
        signal: &SignalRef,
    ) -> Result<BTreeMap<BigUint, CxxrtlSample>> {
        let max_timestamp = self.max_timestamp()?;
        send_command! {
            self,
            CxxrtlCommand::query_interval{
                interval:(CxxrtlTimestamp::zero(), max_timestamp),
                collapse: true,
                items: Some(signal.cxxrtl_repr()),
                item_values_encoding: "base64(u32)",
                daignostics: false
            },
            CommandResponse::query_interval {samples} => {
                Ok(samples.into_iter().map(|s| (s.time.into_femtoseconds(), s)).collect())
            }
        }
    }

    fn signal_values(&mut self, signal: &SignalRef) -> Result<&BTreeMap<BigUint, CxxrtlSample>> {
        if !self.signal_values_cache.contains_key(signal) {
            let value = self.query_signal_values(signal)?;
            self.signal_values_cache.insert(signal.clone(), value);
        }

        Ok(&self.signal_values_cache[signal])
    }

    pub fn query_signal(&mut self, signal: &SignalRef, time: &BigUint) -> Result<QueryResult> {
        let values = self.signal_values(signal)?;
        let current = values.range(..time).next_back();
        let next = values.range(time..).next();

        Ok(QueryResult {
            current: if let Some(c) = current {
                Some((c.0.clone(), c.1.into_signal_value()?))
            } else {
                None
            },
            next: next.map(|c| c.0.clone()),
        })
    }
}
