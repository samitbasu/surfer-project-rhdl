use egui::{Context, Frame, Layout, Margin, TopBottomPanel, Ui};
use emath::Align;
use web_time::{Duration, Instant};

use crate::time::{time_string, timeunit_menu};
use crate::wave_source::draw_progress_information;
use crate::{message::Message, wave_data::WaveData, State};

impl State {
    pub fn add_statusbar_panel(
        &self,
        ctx: &Context,
        waves: &Option<WaveData>,
        msgs: &mut Vec<Message>,
    ) {
        TopBottomPanel::bottom("statusbar")
            .frame(Frame {
                fill: self.config.theme.primary_ui_color.background,
                inner_margin: Margin {
                    left: 5.0,
                    right: 5.0,
                    top: 0.0,
                    bottom: 5.0,
                },
                ..Default::default()
            })
            .show(ctx, |ui| {
                self.draw_statusbar(ui, waves, msgs);
            });
    }

    fn draw_statusbar(&self, ui: &mut Ui, waves: &Option<WaveData>, msgs: &mut Vec<Message>) {
        ui.visuals_mut().override_text_color = Some(self.config.theme.primary_ui_color.foreground);
        ui.with_layout(Layout::left_to_right(Align::RIGHT), |ui| {
            if let Some(waves) = waves {
                ui.label(waves.source.to_string());
                if let Some(datetime) = waves.inner.metadata().date {
                    ui.add_space(10.0);
                    ui.label(format!("Generated: {datetime}"));
                }
            }

            if let Some(state_file) = &self.state_file {
                ui.label(" | ".to_string() + &state_file.to_string_lossy());
            }

            ui.add_space(10.0);
            if let Some(progress_data) = &self.sys.progress_tracker {
                if Instant::now().duration_since(progress_data.started) > Duration::from_millis(100)
                {
                    draw_progress_information(ui, progress_data);
                }
            }
            if let Some(waves) = waves {
                ui.with_layout(Layout::right_to_left(Align::RIGHT), |ui| {
                    if let Some(time) = &waves.cursor {
                        ui.label(time_string(
                            time,
                            &waves.inner.metadata().timescale,
                            &self.wanted_timeunit,
                            &self.get_time_format(),
                        ))
                        .context_menu(|ui| timeunit_menu(ui, msgs, &self.wanted_timeunit));
                        ui.add_space(10.0);
                    }
                    if let Some(undo_op) = &self.sys.undo_stack.last() {
                        ui.label(format!("Undo: {}", undo_op.message));
                        ui.add_space(10.0);
                    }
                    if let Some(count) = &self.count {
                        ui.label(format!("Count: {count}"));
                        ui.add_space(10.0);
                    }
                });
            }
        });
    }
}
