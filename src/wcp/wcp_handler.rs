use std::usize;

use crate::{message::Message, wave_container::VariableRef, State};
use itertools::Itertools;
use log::trace;
use num::BigInt;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum WcpMessage {
    #[serde(rename = "greeting")]
    Greeting {
        version: String,
        commands: Vec<String>,
    },
    #[serde(rename = "command")]
    Command(WcpCommand),
    #[serde(rename = "response")]
    Response { command: String, arguments: Vecs },
    #[serde(rename = "error")]
    Error {
        error: String,
        arguments: Vec<String>,
        message: String,
    },
    #[serde(rename = "event")]
    Event {
        event: String,
        arguments: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "command")]
pub enum WcpCommand {
    #[serde(rename = "get_item_list")]
    GetItemList,
    #[serde(rename = "get_item_info")]
    GetItemInfo { ids: Vec<usize> },
    #[serde(rename = "add_variable")]
    AddVar { names: Vec<String> },
    #[serde(rename = "add_scope")]
    AddScope { scopes: Vec<String> },
    #[serde(rename = "reload")]
    Reload,
    #[serde(rename = "goto")]
    Goto { timestamp: BigInt },
    #[serde(rename = "color")]
    Color { id: usize, color: String },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Vecs {
    Vecs(Vec<String>),
    VecI(Vec<ItemInfo>),
    VecInt(Vec<usize>),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ItemInfo {
    name: String,
    #[serde(rename = "type")]
    t: String,
    id: usize,
}
impl WcpMessage {
    pub fn _client_greeting(version: usize) -> Self {
        Self::Greeting {
            version: version.to_string(),
            commands: vec![],
        }
    }
    pub fn create_greeting(version: usize, commands: Vec<String>) -> Self {
        Self::Greeting {
            version: version.to_string(),
            commands,
        }
    }
    pub fn create_response(command: String, arguments: Vecs) -> Self {
        Self::Response { command, arguments }
    }
    pub fn create_error(error: String, arguments: Vec<String>, message: String) -> Self {
        Self::Error {
            error,
            arguments,
            message,
        }
    }
    pub fn _create_event(event: String, arguments: Vec<String>) -> Self {
        Self::Event { event, arguments }
    }
}

impl State {
    pub fn handle_wcp_commands(&mut self) {
        let Some(receiver) = &mut self.sys.channels.wcp_s2c_receiver else {
            return;
        };

        let mut messages = vec![];
        loop {
            match receiver.try_recv() {
                Ok(command) => {
                    messages.push(command);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    trace!("WCP Command sender disconnected");
                    break;
                }
            }
        }
        for message in messages {
            self.handle_wcp_message(&message);
        }
    }

    fn handle_wcp_message(&mut self, message: &WcpMessage) {
        match message {
            WcpMessage::Command(command) => {
                match command {
                    WcpCommand::GetItemList => {
                        if let Some(waves) = &self.waves {
                            let ids = waves
                                .displayed_items_order
                                .iter()
                                .map(|id| *id)
                                .collect_vec();
                            self.sys.channels.wcp_c2s_sender.as_ref().map(|ch| {
                                ch.send(WcpMessage::create_response(
                                    "get_item_list".to_string(),
                                    Vecs::VecInt(ids),
                                ))
                            });
                        } else {
                            self.sys.channels.wcp_c2s_sender.as_ref().map(|ch| {
                                ch.send(WcpMessage::create_error(
                                    "No waveform loaded".to_string(),
                                    vec![],
                                    "No waveform loaded".to_string(),
                                ))
                            });
                        }
                    }
                    WcpCommand::GetItemInfo { ids } => {
                        let mut items: Vec<ItemInfo> = Vec::new();
                        for id in ids {
                            let item = self.waves.as_ref().unwrap().displayed_items.get(&id);
                            match item {
                                Some(item) => {
                                    let (name, item_type) = match item {
                                        crate::displayed_item::DisplayedItem::Variable(
                                            variable,
                                        ) => (
                                            variable.manual_name.clone().unwrap_or(
                                                variable.variable_ref.full_path_string(),
                                            ),
                                            "Variable".to_string(),
                                        ),
                                        crate::displayed_item::DisplayedItem::Divider(item) => (
                                            item.name
                                                .clone()
                                                .unwrap_or("Name not found!".to_string()),
                                            "Divider".to_string(),
                                        ),
                                        crate::displayed_item::DisplayedItem::Marker(item) => (
                                            item.name
                                                .clone()
                                                .unwrap_or("Name not found!".to_string()),
                                            "Marker".to_string(),
                                        ),
                                        crate::displayed_item::DisplayedItem::TimeLine(item) => (
                                            item.name
                                                .clone()
                                                .unwrap_or("Name not found!".to_string()),
                                            "TimeLine".to_string(),
                                        ),
                                        crate::displayed_item::DisplayedItem::Placeholder(item) => {
                                            (
                                                item.manual_name
                                                    .clone()
                                                    .unwrap_or("Name not found!".to_string()),
                                                "Placeholder".to_string(),
                                            )
                                        }
                                    };
                                    items.push(ItemInfo {
                                        name,
                                        t: item_type,
                                        id: *id,
                                    });
                                }
                                None => (),
                            };
                        }
                        self.sys.channels.wcp_c2s_sender.as_ref().map(|ch| {
                            ch.send(WcpMessage::create_response(
                                "get_item_info".to_string(),
                                Vecs::VecI(items),
                            ))
                        });
                    }
                    WcpCommand::AddVar { names } => {
                        let ids = names
                            .iter()
                            .map(|variable| {
                                self.waves.as_mut().and_then(|waves| {
                                    waves.add_variable(
                                        &self.sys.translators,
                                        &VariableRef::from_hierarchy_string(&variable),
                                    )
                                })
                            })
                            .map(|id| id.map(|id| (id).to_string()).unwrap_or("None".to_string()))
                            .collect();

                        self.sys.channels.wcp_c2s_sender.as_ref().map(|ch| {
                            ch.send(WcpMessage::create_response(
                                "add_variables".to_string(),
                                Vecs::Vecs(ids),
                            ))
                        });
                        self.invalidate_draw_commands();
                    }
                    WcpCommand::AddScope { scopes: _ } => todo!(),
                    WcpCommand::Reload => {
                        self.update(Message::ReloadWaveform(false));
                        self.sys.channels.wcp_c2s_sender.as_ref().map(|ch| {
                            ch.send(WcpMessage::create_response(
                                "reload".to_string(),
                                Vecs::Vecs(vec![]),
                            ))
                        });
                    }
                    WcpCommand::Goto { timestamp } => {
                        self.update(Message::GoToTime(Some(timestamp.clone()), 0));
                        self.sys.channels.wcp_c2s_sender.as_ref().map(|ch| {
                            ch.send(WcpMessage::create_response(
                                "goto".to_string(),
                                Vecs::Vecs(vec![]),
                            ))
                        });
                    }
                    WcpCommand::Color { id, color } => {
                        let id = id.saturating_sub(1);
                        println!("color: {}", color);
                        self.update(Message::ItemColorChange(Some(id), Some(color.clone())));

                        self.sys.channels.wcp_c2s_sender.as_ref().map(|ch| {
                            ch.send(WcpMessage::create_response(
                                "color".to_string(),
                                Vecs::Vecs(vec![]),
                            ))
                        });
                    }
                };
            }
            _ => {
                self.sys.channels.wcp_c2s_sender.as_ref().map(|ch| {
                    ch.send(WcpMessage::create_error(
                        "Illegal command".to_string(),
                        vec![],
                        "Illegal command".to_string(),
                    ))
                });
            }
        }
    }
}
