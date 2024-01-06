use eframe::egui::{Context, Frame, Layout, TopBottomPanel, Ui};
use eframe::emath::Align;

use crate::time::{time_string, timeunit_menu};
use crate::{message::Message, wave_data::WaveData, State};

impl State {
    pub fn add_statusbar_panel(&self, ctx: &Context, waves: &WaveData, msgs: &mut Vec<Message>) {
        TopBottomPanel::bottom("statusbar")
            .frame(Frame {
                fill: self.config.theme.primary_ui_color.background,
                ..Default::default()
            })
            .show(ctx, |ui| {
                self.draw_statusbar(ui, waves, msgs);
            });
    }

    fn draw_statusbar(&self, ui: &mut Ui, waves: &WaveData, msgs: &mut Vec<Message>) {
        ui.visuals_mut().override_text_color = Some(self.config.theme.primary_ui_color.foreground);
        ui.with_layout(Layout::left_to_right(Align::RIGHT), |ui| {
            ui.add_space(10.0);
            if self.show_wave_source {
                ui.label(&waves.source.to_string());
                if let Some(datetime) = waves.inner.metadata().date {
                    ui.add_space(10.0);
                    ui.label(format!("Generated: {datetime}"));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::RIGHT), |ui| {
                if let Some(time) = &waves.cursor {
                    ui.label(time_string(
                        time,
                        &waves.inner.metadata().timescale,
                        &self.wanted_timeunit,
                    ))
                    .context_menu(|ui| timeunit_menu(ui, msgs, &self.wanted_timeunit));
                    ui.add_space(10.0)
                }
                if let Some(count) = &self.count {
                    ui.label(format!("Count: {}", count));
                    ui.add_space(20.0);
                }
            });
        });
    }
}
