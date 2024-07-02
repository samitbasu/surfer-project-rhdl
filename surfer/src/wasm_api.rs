use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;

use lazy_static::lazy_static;
use log::{error, info, warn};
use wasm_bindgen::prelude::*;

use crate::Message;
use crate::State;

lazy_static! {
    pub static ref MESSAGE_QUEUE: Mutex<Vec<Message>> = Mutex::new(vec![]);
    pub static ref EGUI_CONTEXT: RwLock<Option<Arc<eframe::egui::Context>>> = RwLock::new(None);
}

#[wasm_bindgen]
pub fn inject_message(message: &str) {
    let deser = serde_json::from_str(message);

    match deser {
        Ok(message) => {
            MESSAGE_QUEUE.lock().unwrap().push(message);

            if let Some(ctx) = EGUI_CONTEXT.read().unwrap().as_ref() {
                ctx.request_repaint();
            } else {
                warn!("Attempted to request surfer repaint but surfer has not given us EGUI_CONTEXT yet")
            }
        }
        Err(e) => {
            error!("{e:#?}")
        }
    }
}

impl State {
    pub(crate) fn handle_wasm_external_messages(&mut self) {
        while let Some(msg) = MESSAGE_QUEUE.lock().unwrap().pop() {
            self.update(msg);
        }
    }
}
