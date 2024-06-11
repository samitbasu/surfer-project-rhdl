use std::{borrow::Cow, sync::Mutex};

use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder, TableRow};
use log::{Level, Log, Record};

use crate::{message::Message, State};

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
    pub fn records(&self) -> Vec<LogMessage<'_>> {
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

impl State {
    pub fn draw_log_window(&self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        egui::Window::new("Logs")
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .show(ctx, |ui| {
                ui.style_mut().wrap = Some(false);

                egui::ScrollArea::new([true, false]).show(ui, |ui| {
                    TableBuilder::new(ui)
                        .column(Column::auto().resizable(true))
                        .column(Column::remainder())
                        .vscroll(true)
                        .stick_to_bottom(true)
                        .header(20.0, |mut header| {
                            header.col(|ui| {
                                ui.heading("Level");
                            });
                            header.col(|ui| {
                                ui.heading("Message");
                            });
                        })
                        .body(|body| {
                            let records = EGUI_LOGGER.records();
                            let heights = records
                                .iter()
                                .map(|record| {
                                    let height = record.msg.lines().count() as f32;

                                    height * 15.
                                })
                                .collect::<Vec<_>>();

                            body.heterogeneous_rows(heights.into_iter(), |mut row: TableRow| {
                                let record = &records[row.index()];
                                row.col(|ui| {
                                    let (color, text) = match record.level {
                                        log::Level::Error => (Color32::RED, "Error"),
                                        log::Level::Warn => (Color32::YELLOW, "Warn"),
                                        log::Level::Info => (Color32::GREEN, "Info"),
                                        log::Level::Debug => (Color32::BLUE, "Debug"),
                                        log::Level::Trace => (Color32::GRAY, "Trace"),
                                    };

                                    ui.colored_label(color, text);
                                });
                                row.col(|ui| {
                                    ui.label(RichText::new(record.msg.clone()).monospace());
                                });
                            });
                        })
                })
            });
        if !open {
            msgs.push(Message::SetLogsVisible(false))
        }
    }
}