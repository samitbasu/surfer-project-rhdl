use eframe::{
    egui::{self, Layout, RichText},
    emath::Align,
};
use material_icons::{icon_to_char, Icon};

use crate::{message::Message, wave_source::OpenMode, State};

fn add_toolbar_button(
    ui: &mut egui::Ui,
    msgs: &mut Vec<Message>,
    icon: Icon,
    hover_text: &str,
    message: Message,
) {
    ui.button(RichText::new(icon_to_char(icon).to_string()).heading())
        .on_hover_text(hover_text)
        .clicked()
        .then(|| msgs.push(message));
}

impl State {
    pub fn draw_toolbar(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
        ui.with_layout(Layout::left_to_right(Align::LEFT), |ui| {
            add_toolbar_button(
                ui,
                msgs,
                Icon::FileOpen,
                "Open file...",
                Message::OpenFileDialog(OpenMode::Open),
            );
            add_toolbar_button(
                ui,
                msgs,
                Icon::Download,
                "Open URL...",
                Message::SetUrlEntryVisible(true),
            );
            ui.separator();
            add_toolbar_button(
                ui,
                msgs,
                Icon::ZoomIn,
                "Zoom in",
                Message::CanvasZoom {
                    mouse_ptr_timestamp: None,
                    delta: 0.5,
                },
            );
            add_toolbar_button(
                ui,
                msgs,
                Icon::ZoomOut,
                "Zoom out",
                Message::CanvasZoom {
                    mouse_ptr_timestamp: None,
                    delta: 2.0,
                },
            );
            add_toolbar_button(ui, msgs, Icon::FitScreen, "Zoom to fit", Message::ZoomToFit);
            add_toolbar_button(ui, msgs, Icon::FirstPage, "Go to start", Message::GoToStart);
            add_toolbar_button(ui, msgs, Icon::LastPage, "Go to end", Message::GoToEnd);
            ui.separator();
            add_toolbar_button(
                ui,
                msgs,
                Icon::SpaceBar,
                "Add divider",
                Message::AddDivider(String::new()),
            );
            add_toolbar_button(
                ui,
                msgs,
                Icon::MoreTime,
                "Add timeline",
                Message::AddTimeLine,
            );
        });
    }
}
