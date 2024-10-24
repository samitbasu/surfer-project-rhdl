use crate::{
    displayed_item::{DisplayedItem, DisplayedItemIndex, DisplayedItemRef},
    message::Message,
    wave_container::{ScopeRefExt, VariableRef, VariableRefExt},
    wave_data::WaveData,
    wave_source::{string_to_wavesource, LoadOptions, WaveSource},
    State,
};

use itertools::Itertools;
use log::{trace, warn};
use num::BigInt;
use serde::{Deserialize, Serialize};
use surfer_translation_types::ScopeRef;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "command")]
pub enum WcpCommand {
    #[serde(rename = "get_item_list")]
    GetItemList,
    #[serde(rename = "get_item_info")]
    GetItemInfo { ids: Vec<String> },
    #[serde(rename = "set_item_color")]
    SetItemColor { id: String, color: String },
    #[serde(rename = "add_variables")]
    AddVariables { names: Vec<String> },
    #[serde(rename = "add_scope")]
    AddScope { scope: String },
    #[serde(rename = "reload")]
    Reload,
    #[serde(rename = "set_viewport_to")]
    SetViewportTo { timestamp: BigInt },
    #[serde(rename = "remove_items")]
    RemoveItems { ids: Vec<String> },
    #[serde(rename = "focus_item")]
    FocusItem { id: String },
    #[serde(rename = "clear")]
    Clear,
    #[serde(rename = "load")]
    Load { source: String },
    #[serde(rename = "zoom_to_fit")]
    ZoomToFit { viewport_idx: usize },
    #[serde(rename = "shutdown")]
    Shutdown,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Vecs {
    String(Vec<String>),
    Info(Vec<ItemInfo>),
    Int(Vec<usize>),
    Tuple(Vec<(String, usize)>),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ItemInfo {
    name: String,
    #[serde(rename = "type")]
    t: String,
    id: usize,
}

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
impl WcpMessage {
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
                            let ids = self
                                .get_displayed_items(waves)
                                .iter()
                                .map(|i| format!("{}", i.0))
                                .collect_vec();
                            self.send_response(command, Vecs::String(ids));
                        } else {
                            self.send_error("No waveform loaded", vec![], "No waveform loaded");
                        }
                    }
                    WcpCommand::GetItemInfo { ids } => {
                        let mut items: Vec<ItemInfo> = Vec::new();
                        for id in ids {
                            if let Ok(id) = usize::from_str_radix(id, 10) {
                                let item = self
                                    .waves
                                    .as_ref()
                                    .unwrap()
                                    .displayed_items
                                    .get(&DisplayedItemRef(id));

                                if let Some(item) = item {
                                    let (name, item_type) = match item {
                                        DisplayedItem::Variable(var) => (
                                            var.manual_name
                                                .clone()
                                                .unwrap_or(var.display_name.clone()),
                                            "Variable".to_string(),
                                        ),
                                        DisplayedItem::Divider(item) => (
                                            item.name
                                                .clone()
                                                .unwrap_or("Name not found!".to_string()),
                                            "Divider".to_string(),
                                        ),
                                        DisplayedItem::Marker(item) => (
                                            item.name
                                                .clone()
                                                .unwrap_or("Name not found!".to_string()),
                                            "Marker".to_string(),
                                        ),
                                        DisplayedItem::TimeLine(item) => (
                                            item.name
                                                .clone()
                                                .unwrap_or("Name not found!".to_string()),
                                            "TimeLine".to_string(),
                                        ),
                                        DisplayedItem::Placeholder(item) => (
                                            item.manual_name
                                                .clone()
                                                .unwrap_or("Name not found!".to_string()),
                                            "Placeholder".to_string(),
                                        ),
                                        DisplayedItem::Stream(item) => (
                                            item.manual_name
                                                .clone()
                                                .unwrap_or(item.display_name.clone()),
                                            "Stream".to_string(),
                                        ),
                                    };
                                    items.push(ItemInfo {
                                        name,
                                        t: item_type,
                                        id: id,
                                    });
                                }
                            }
                        }
                        self.send_response(command, Vecs::Info(items));
                    }
                    WcpCommand::AddVariables { names } => {
                        if self.waves.is_some() {
                            self.save_current_canvas(format!("Add {} variables", names.len()));
                        }
                        if let Some(waves) = self.waves.as_mut() {
                            let variable_refs = names
                                .iter()
                                .map(|n| VariableRef::from_hierarchy_string(n))
                                .collect_vec();
                            let (cmd, ids) =
                                waves.add_variables(&self.sys.translators, variable_refs);
                            if let Some(cmd) = cmd {
                                self.load_variables(cmd);
                            }
                            self.send_response(
                                command,
                                Vecs::String(
                                    ids.iter().map(|id| format!("{}", id.0)).collect_vec(),
                                ),
                            );
                            self.invalidate_draw_commands();
                        } else {
                            self.send_error(
                                "add_variables",
                                vec![],
                                "Can't add signals. No waveform loaded",
                            )
                        }
                    }
                    WcpCommand::AddScope { scope } => {
                        if self.waves.is_some() {
                            self.save_current_canvas(format!("Add scope {}", scope));
                        }
                        if let Some(waves) = self.waves.as_mut() {
                            let scope = ScopeRef::from_hierarchy_string(scope);

                            let variables =
                                waves.inner.as_waves().unwrap().variables_in_scope(&scope);
                            let (cmd, ids) = waves.add_variables(&self.sys.translators, variables);
                            if let Some(cmd) = cmd {
                                self.load_variables(cmd);
                            }
                            self.send_response(
                                command,
                                Vecs::String(
                                    ids.iter().map(|id| format!("{}", id.0)).collect_vec(),
                                ),
                            );
                            self.invalidate_draw_commands();
                        } else {
                            self.send_error(
                                "scope_add",
                                vec![],
                                format!("No waveform loaded").as_str(),
                            );
                        }
                    }
                    WcpCommand::Reload => {
                        self.update(Message::ReloadWaveform(false));
                        self.send_response(command, Vecs::String(vec![]));
                    }
                    WcpCommand::SetViewportTo { timestamp } => {
                        self.update(Message::GoToTime(Some(timestamp.clone()), 0));
                        self.send_response(command, Vecs::String(vec![]));
                    }
                    WcpCommand::SetItemColor { id, color } => {
                        let Some(waves) = &self.waves else {
                            self.send_error(
                                "set_item_color",
                                vec![],
                                format!("No waveform loaded").as_str(),
                            );
                            return;
                        };
                        if let Ok(id) = usize::from_str_radix(id, 10) {
                            if let Some(idx) = waves
                                .displayed_items_order
                                .iter()
                                .find_position(|&list_id| list_id.0 == id)
                            {
                                self.update(Message::ItemColorChange(
                                    Some(DisplayedItemIndex(idx.0)),
                                    Some(color.clone()),
                                ));
                                self.send_response(command, Vecs::String(vec![]))
                            } else {
                                self.send_error(
                                    "set_item_color",
                                    vec![],
                                    format!("Item {id} not found").as_str(),
                                );
                            }
                        } else {
                            self.send_error(
                                "set_item_color",
                                vec![],
                                format!("{id} is not valid Surfer id").as_str(),
                            );
                        }
                    }
                    WcpCommand::RemoveItems { ids } => {
                        let Some(waves) = self.waves.as_mut() else {
                            self.send_error("remove_items", vec![], "No waveform loaded");
                            return;
                        };
                        let mut msgs = vec![];
                        for id in ids {
                            if let Ok(id) = usize::from_str_radix(id, 10) {
                                if let Some(idx) = waves
                                    .displayed_items_order
                                    .iter()
                                    .find_position(|&list_id| list_id.0 == id)
                                {
                                    msgs.push(Message::RemoveItems(vec![*idx.1]));
                                }
                            }
                        }
                        self.update(Message::Batch(msgs));

                        self.send_response(command, Vecs::Int(vec![]));
                    }
                    WcpCommand::FocusItem { id } => {
                        let Some(waves) = &self.waves else {
                            self.send_error("remove_items", vec![], "No waveform loaded");
                            return;
                        };
                        if let Ok(id) = usize::from_str_radix(id, 10) {
                            if let Some(idx) = waves
                                .displayed_items_order
                                .iter()
                                .find_position(|&list_id| list_id.0 == id)
                            {
                                self.update(Message::FocusItem(DisplayedItemIndex(idx.0)));

                                self.send_response(command, Vecs::Int(vec![]));
                            } else {
                                self.send_error(
                                    "focus_item",
                                    vec![],
                                    format!("No item with ID {id}").as_str(),
                                );
                            }
                        }
                    }
                    WcpCommand::Clear => {
                        match &self.waves {
                            Some(wave) => {
                                self.update(Message::RemoveItems(self.get_displayed_items(wave)))
                            }
                            None => (),
                        }

                        self.send_response(command, Vecs::Int(vec![]));
                    }
                    WcpCommand::Load { source } => {
                        self.sys.wcp_server_load_outstanding = true;
                        match string_to_wavesource(source) {
                            WaveSource::Url(url) => {
                                self.update(Message::LoadWaveformFileFromUrl(
                                    url,
                                    LoadOptions::clean(),
                                ));
                            }
                            WaveSource::File(file) => {
                                let msg = match file.extension().unwrap() {
                                    // "ftr" => {
                                    // Message::LoadTransactionFile(file, LoadOptions::clean())
                                    // }
                                    _ => Message::LoadFile(file, LoadOptions::clean()),
                                };
                                self.update(msg);
                            }
                            _ => {
                                self.send_error(
                                    "load",
                                    vec![],
                                    format!("{source} is not legal wave source").as_str(),
                                );
                            }
                        }
                    }
                    WcpCommand::ZoomToFit { viewport_idx } => {
                        self.update(Message::ZoomToFit {
                            viewport_idx: *viewport_idx,
                        });
                        self.send_response(command, Vecs::Int(vec![]));
                    }
                    WcpCommand::Shutdown => {
                        warn!("WCP Shutdown message should not reach this place")
                    }
                };
            }
            _ => {
                self.send_error("Illegal command", vec![], "Illegal command");
            }
        }
    }
    fn send_response(&self, command: &WcpCommand, result: Vecs) {
        let serde_json::Value::Object(tag) = serde_json::to_value(command).unwrap() else {
            self.send_error(
                "Could not serialize command",
                vec![],
                "try sending a valid command",
            );
            return;
        };
        let Some(serde_json::Value::String(command)) = tag.get("command") else {
            self.send_error(
                "Command tag does not have a value",
                vec![],
                "try sending a valid command",
            );
            return;
        };

        self.sys
            .channels
            .wcp_c2s_sender
            .as_ref()
            .map(|ch| ch.send(WcpMessage::create_response(command.clone(), result)));
    }

    fn send_error(&self, error: &str, arguments: Vec<String>, message: &str) {
        self.sys.channels.wcp_c2s_sender.as_ref().map(|ch| {
            ch.send(WcpMessage::create_error(
                error.to_string(),
                arguments,
                message.to_string(),
            ))
        });
    }

    fn get_displayed_items(&self, waves: &WaveData) -> Vec<DisplayedItemRef> {
        waves.displayed_items_order.iter().copied().collect_vec()
    }
}
