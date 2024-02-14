use lazy_static::lazy_static;
use log::{error, info};
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

use crate::Message;
use crate::State;

lazy_static! {
    pub static ref MESSAGE_QUEUE: Mutex<Vec<Message>> = Mutex::new(vec![]);
}

// Export a `greet` function from Rust to JavaScript, that alerts a
// hello message.
#[wasm_bindgen]
pub fn inject_message(message: &str) {
    info!("Processing message {message} from wasm");
    let deser = serde_json::from_str(message);

    match deser {
        Ok(message) => MESSAGE_QUEUE.lock().unwrap().push(message),
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
