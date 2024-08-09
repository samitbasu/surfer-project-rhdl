use egui::{Context, Window};

use crate::{message::Message, wave_source::LoadOptions, State};

impl State {
    pub fn draw_surver_file_window(
        &self,
        file_list: &Vec<String>,
        ctx: &Context,
        msgs: &mut Vec<Message>,
    ) {
        let mut open = true;

        Window::new("Select wave file")
            .collapsible(true)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    for (i, file) in file_list.iter().enumerate() {
                        if ui.label(file).clicked() {
                            msgs.push(Message::SetServerFileWindowVisible(false));
                            msgs.push(Message::SetSelectedServerFile(Some(i)));
                            msgs.push(Message::LoadWaveformFileFromUrl(
                                self.surver_url.clone().unwrap(),
                                LoadOptions::clean(),
                            ));
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        msgs.push(Message::SetServerFileWindowVisible(false));
                    }
                    if ui.button("Select").clicked() {
                        msgs.push(Message::SetSelectedServerFile(Some(0)));
                        msgs.push(Message::SetServerFileWindowVisible(false));
                        // msgs.push(Message::LoadWaveformFileFromUrl(url, load_options));
                    }
                })
            });
        if !open {
            msgs.push(Message::SetServerFileWindowVisible(false))
        }
    }
}
