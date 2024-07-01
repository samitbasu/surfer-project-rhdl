use log::{Level, Log, Record};
use std::{borrow::Cow, sync::Mutex};

pub static SURVER_LOGGER: SurverLogger = SurverLogger {
    records: Mutex::new(vec![]),
};

#[derive(Clone)]
pub struct LogMessage<'a> {
    pub msg: Cow<'a, str>,
    pub level: Level,
}

pub struct SurverLogger<'a> {
    records: Mutex<Vec<LogMessage<'a>>>,
}

impl Log for SurverLogger<'_> {
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
