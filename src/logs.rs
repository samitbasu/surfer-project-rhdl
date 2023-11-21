use std::{borrow::Cow, sync::Mutex};

use log::{Level, Log, Record};

pub static EGUI_LOGGER: EguiLogger = EguiLogger {
    records: Mutex::new(vec![]),
};

#[derive(Clone)]
pub struct LogMessage<'a> {
    pub msg: Cow<'a, str>,
    pub level: Level,
}

pub struct EguiLogger<'a> {
    records: Mutex<Vec<LogMessage<'a>>>,
}

impl EguiLogger<'_> {
    pub fn records<'a>(&'a self) -> Vec<LogMessage<'a>> {
        self.records
            .lock()
            .expect("Failed to lock logger. Thread poisoned?")
            .to_vec()
    }
}

impl Log for EguiLogger<'_> {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        self.records
            .lock()
            .expect("Failed to lock logger. Thread poisoned?")
            .push(LogMessage {
                msg: format!("{}", record.args()).into(),
                level: record.level(),
            })
    }

    fn flush(&self) {}
}
