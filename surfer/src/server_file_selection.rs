use egui::{Context, ScrollArea, Window};

use crate::{message::Message, wave_source::LoadOptions, State};

impl State {
    pub fn draw_surver_file_window(
        &self,
        file_list: &Vec<String>,
        ctx: &Context,
        msgs: &mut Vec<Message>,
    ) {
        let mut open = true;

        let selected_file_idx = *self.sys.selected_surver_file.borrow_mut();

        Window::new("Select wave file")
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ScrollArea::both().id_source("file_list").show(ui, |ui| {
                    ui.vertical(|ui| {
                        for (i, file) in file_list.iter().enumerate() {
                            if ui
                                .selectable_label(Some(i) == selected_file_idx, file)
                                .clicked()
                            {
                                *self.sys.selected_surver_file.borrow_mut() = Some(i);
                            }
                        }
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        msgs.push(Message::SetServerFileWindowVisible(false));
                    }
                    if ui.button("Select").clicked() {
                        if let Some(file_idx) = selected_file_idx {
                            msgs.push(Message::SetServerFileWindowVisible(false));
                            msgs.push(Message::SetSelectedServerFile(Some(file_idx)));
                            msgs.push(Message::LoadWaveformFileFromUrl(
                                self.surver_url.clone().unwrap(),
                                LoadOptions::clean(),
                            ));
                        }
                    }
                })
            });
        if !open {
            msgs.push(Message::SetServerFileWindowVisible(false))
        }
    }
}
