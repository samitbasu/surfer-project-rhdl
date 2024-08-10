use egui::{Context, ScrollArea, TextWrapMode, Window};

use crate::{message::Message, wave_source::LoadOptions, State};

impl State {
    pub fn draw_surver_file_window(
        &self,
        file_list: &[String],
        ctx: &Context,
        msgs: &mut Vec<Message>,
    ) {
        let mut open = true;

        let selected_file_idx = *self.sys.surver_selected_file.borrow_mut();

        Window::new("Select wave file")
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ScrollArea::both().id_source("file_list").show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        for (i, file) in file_list.iter().enumerate() {
                            if ui
                                .selectable_label(Some(i) == selected_file_idx, file)
                                .clicked()
                            {
                                *self.sys.surver_selected_file.borrow_mut() = Some(i);
                            }
                        }
                    });
                });
                if self.surver_file_idx.is_some() {
                    ui.separator();
                    ui.checkbox(
                        &mut self.sys.surver_keep_variables.borrow_mut(),
                        "Keep variables",
                    );
                    ui.checkbox(
                        &mut self.sys.surver_keep_unavailable.borrow_mut(),
                        "Keep unavailable variables",
                    );
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        msgs.push(Message::SetServerFileWindowVisible(false));
                    }
                    if ui.button("Select").clicked() {
                        if let Some(file_idx) = selected_file_idx {
                            let keep_variables = *self.sys.surver_keep_variables.borrow_mut();
                            let keep_unavailable = *self.sys.surver_keep_unavailable.borrow_mut();

                            msgs.push(Message::SetServerFileWindowVisible(false));
                            msgs.push(Message::SetSelectedServerFile(Some(file_idx)));
                            msgs.push(Message::LoadWaveformFileFromUrl(
                                self.surver_url.clone().unwrap(),
                                LoadOptions {
                                    keep_variables,
                                    keep_unavailable,
                                },
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
