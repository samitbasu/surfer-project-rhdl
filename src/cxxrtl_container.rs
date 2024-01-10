use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use color_eyre::{
    eyre::{bail, Context},
    Result,
};
use itertools::Itertools;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};

use crate::wave_container::{ModuleRef, SignalRef};

impl ModuleRef {
    fn cxxrtl_repr(&self) -> String {
        self.0.iter().join(" ")
    }
}

#[derive(Serialize)]
#[serde(tag = "command")]
#[allow(non_camel_case_types)]
enum CxxrtlCommand {
    list_scopes,
    list_items { scope: Option<String> },
}

#[derive(Serialize)]
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
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[allow(non_camel_case_types)]
enum SCMessage {
    greeting {
        version: i64,
        commands: Vec<String>,
        events: Vec<String>,
        features: Features,
    },
    response(CommandResponse),
}

pub struct CxxrtlContainer {
    stream: TcpStream,
    read_buf: VecDeque<u8>,

    scopes_cache: Option<HashMap<ModuleRef, CxxrtlScope>>,
    module_item_cache: HashMap<ModuleRef, HashMap<SignalRef, CxxrtlItem>>,
}

impl CxxrtlContainer {
    pub fn new(addr: &str) -> Result<Self> {
        let mut stream =
            TcpStream::connect(addr).with_context(|| format!("Failed to connect to {addr}"))?;

        stream.write_all(
            serde_json::to_string(&CSMessage::greeting { version: 0 })
                .with_context(|| format!("Failed to encode greeting message"))?
                .as_bytes(),
        )?;
        stream.write_all(&[b'\0'])?;

        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .with_context(|| "Failed to set stream timeout")?;

        let mut result = Self {
            stream,
            read_buf: VecDeque::new(),
            scopes_cache: None,
            module_item_cache: HashMap::new(),
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

        Ok(decoded)
    }

    fn fetch_scopes(&mut self) -> Result<HashMap<String, CxxrtlScope>> {
        self.send_message(CSMessage::command(CxxrtlCommand::list_scopes))?;

        let response = self
            .read_one_message()
            .context("failed to read scope response")?;

        if let SCMessage::response(CommandResponse::list_scopes { scopes }) = response {
            Ok(scopes)
        } else {
            bail!("Did not get a scope response from cxxrtl. Got {response:?} instead")
        }
    }

    fn fetch_items_in_module(&mut self, module: &ModuleRef) -> Result<HashMap<String, CxxrtlItem>> {
        self.send_message(CSMessage::command(CxxrtlCommand::list_items {
            scope: Some(module.cxxrtl_repr()),
        }))?;

        let response = self
            .read_one_message()
            .context("failed to read scope response")?;

        if let SCMessage::response(CommandResponse::list_items { items }) = response {
            Ok(items)
        } else {
            bail!("Did not get a scope response from cxxrtl. Got {response:?} instead")
        }
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
                self.module_item_cache.insert(
                    module.clone(),
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
                                        path: module.clone(),
                                        name: sp.last().unwrap().to_string(),
                                    },
                                    v,
                                ))
                            }
                        })
                        .collect(),
                );
            }
        }

        self.module_item_cache
            .get(module)
            .map(|items| items.iter().map(|(k, _)| k.clone()).collect())
            .unwrap_or_default()
    }
}
