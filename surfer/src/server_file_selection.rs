use egui::{Context, Window};

use crate::{message::Message, wave_source::LoadOptions, State};

impl State {
    pub fn draw_server_file_window(
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
                            msgs.push(Message::SetSelectedServerFile(Some(i)));
                            msgs.push(Message::LoadWaveformFileFromUrl(
                                self.server_url.clone().unwrap(),
                                LoadOptions::clean(),
                            ));
                            msgs.push(Message::SetServerFileWindowVisible(false));
                        }
                    }
                });
                ui.add_space(15.);
                ui.horizontal(|ui| {
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
