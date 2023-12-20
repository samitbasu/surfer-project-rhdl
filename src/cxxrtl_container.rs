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
use log::info;
use serde::{Deserialize, Serialize};

use crate::wave_container::ModuleRef;

#[derive(Serialize)]
#[serde(tag = "command")]
#[allow(non_camel_case_types)]
enum CxxrtlCommand {
    list_scopes,
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
    src: String,
    // TODO: More stuff
}

#[derive(Deserialize, Debug)]
#[serde(tag = "command")]
#[allow(non_camel_case_types)]
enum CommandResponse {
    list_scopes {
        scopes: HashMap<String, CxxrtlScope>,
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

    scopes_cache: Option<HashMap<String, CxxrtlScope>>,
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

    fn scopes(&mut self) -> Option<&HashMap<String, CxxrtlScope>> {
        if self.scopes_cache.is_none() {
            self.scopes_cache = self
                .fetch_scopes()
                .map_err(|e| info!("Failed to get modules: {e:#?}"))
                .ok();

            info!("fetched scopes, cache is now {:?}", self.scopes_cache);
        }

        self.scopes_cache.as_ref()
    }

    pub fn modules(&mut self) -> Vec<ModuleRef> {
        if let Some(scopes) = &self.scopes() {
            scopes
                .iter()
                .map(|(k, _)| ModuleRef(k.split(" ").map(|s| s.to_string()).collect::<Vec<_>>()))
                .collect()
        } else {
            vec![]
        }
    }

    pub fn root_modules(&mut self) -> Vec<ModuleRef> {
        if let Some(scopes) = &self.scopes() {
            scopes
                .iter()
                .filter(|(k, _)| k.is_empty())
                .map(|(k, _)| ModuleRef(k.split(" ").map(|s| s.to_string()).collect::<Vec<_>>()))
                .collect()
        } else {
            vec![]
        }
    }
}
