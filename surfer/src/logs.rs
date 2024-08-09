use std::{borrow::Cow, sync::Mutex};

use color_eyre::Result;
use ecolor::Color32;
use egui::{self, RichText, TextWrapMode};
use egui_extras::{Column, TableBuilder, TableRow};
use log::{Level, Log, Record};

use crate::{message::Message, State};

pub static EGUI_LOGGER: EguiLogger = EguiLogger {
    records: Mutex::new(vec![]),
};

#[macro_export]
macro_rules! try_log_error {
    ($expr:expr, $what:expr $(,)?) => {
        if let Err(e) = $expr {
            error!("{}: {}", $what, e)
        }
    };
}

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
            });
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
                ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);

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
                        });
                })
            });
        if !open {
            msgs.push(Message::SetLogsVisible(false));
        }
    }
}

pub fn setup_logging(platform_logger: fern::Dispatch) -> Result<()> {
    let egui_log_config = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .level_for("surfer", log::LevelFilter::Trace)
        .format(move |out, message, _record| out.finish(format_args!(" {message}")))
        .chain(&EGUI_LOGGER as &(dyn log::Log + 'static));

    fern::Dispatch::new()
        .chain(platform_logger)
        .chain(egui_log_config)
        .apply()?;
    Ok(())
}

/// Starts the logging and error handling. Can be used by unittests to get more insights.
#[cfg(not(target_arch = "wasm32"))]
pub fn start_logging() -> Result<()> {
    let colors = fern::colors::ColoredLevelConfig::new()
        .error(fern::colors::Color::Red)
        .warn(fern::colors::Color::Yellow)
        .info(fern::colors::Color::Green)
        .debug(fern::colors::Color::Blue)
        .trace(fern::colors::Color::White);

    let stdout_config = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .level_for("surfer", log::LevelFilter::Trace)
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}] {}",
                colors.color(record.level()),
                message
            ));
        })
        .chain(std::io::stdout());
    setup_logging(stdout_config)?;

    color_eyre::install()?;
    Ok(())
}
