use crate::message::Message;
use crate::State;
use ecolor::Color32;
use egui::{Layout, RichText};
use emath::Align;

#[derive(Debug, Default, Copy, Clone)]
pub struct ReloadWaveformDialog {
    /// `true` to persist the setting returned by the dialog.
    do_not_show_again: bool,
}

impl State {
    /// Draw a dialog that asks for user confirmation before re-loading a file.
    /// This is triggered by a file loading event from disk.
    pub(crate) fn draw_reload_waveform_dialog(
        &self,
        ctx: &egui::Context,
        dialog: &ReloadWaveformDialog,
        msgs: &mut Vec<Message>,
    ) {
        let mut do_not_show_again = dialog.do_not_show_again;
        egui::Window::new("File Change")
            .auto_sized()
            .collapsible(false)
            .fixed_pos(ctx.available_rect().center())
            .show(ctx, |ui| {
                let label = ui.label(RichText::new("File on disk has changed. Reload?").heading());
                ui.set_width(label.rect.width());
                ui.add_space(5.0);
                ui.checkbox(
                    &mut do_not_show_again,
                    "Remember my decision for this session",
                );
                ui.add_space(14.0);
                ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                    // Sets the style when focused
                    ui.style_mut().visuals.widgets.active.weak_bg_fill = Color32::BLUE;
                    let reload_button = ui.button("Reload");
                    let leave_button = ui.button("Leave");
                    ctx.memory_mut(|mem| {
                        if mem.focused() != Some(reload_button.id)
                            && mem.focused() != Some(leave_button.id)
                        {
                            mem.request_focus(reload_button.id)
                        }
                    });

                    if reload_button.clicked() {
                        msgs.push(Message::CloseReloadWaveformDialog {
                            reload_file: true,
                            do_not_show_again,
                        });
                    } else if leave_button.clicked() {
                        msgs.push(Message::CloseReloadWaveformDialog {
                            reload_file: false,
                            do_not_show_again,
                        });
                    } else if do_not_show_again != dialog.do_not_show_again {
                        msgs.push(Message::UpdateReloadWaveformDialog(ReloadWaveformDialog {
                            do_not_show_again,
                        }));
                    }
                });
            });
    }
}
