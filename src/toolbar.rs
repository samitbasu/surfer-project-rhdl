use eframe::egui::{Button, Context, Layout, RichText, TopBottomPanel, Ui};
use eframe::emath::Align;
use eframe::epaint::Vec2;
use egui_remixicon::icons;

use crate::{
    message::Message,
    wave_data::{PER_SCROLL_EVENT, SCROLL_EVENTS_PER_PAGE},
    wave_source::OpenMode,
    State,
};

fn add_toolbar_button(
    ui: &mut Ui,
    msgs: &mut Vec<Message>,
    icon_string: &str,
    hover_text: &str,
    message: Message,
    enabled: bool,
) {
    let button = Button::new(RichText::new(icon_string).heading()).frame(false);
    ui.add_enabled(enabled, button)
        .on_hover_text(hover_text)
        .clicked()
        .then(|| msgs.push(message));
}

impl State {
    pub fn add_toolbar_panel(&self, ctx: &Context, msgs: &mut Vec<Message>) {
        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            self.draw_toolbar(ui, msgs);
        });
    }

    fn draw_toolbar(&self, ui: &mut Ui, msgs: &mut Vec<Message>) {
        let wave_loaded = self.waves.is_some();
        let item_selected = wave_loaded && self.waves.as_ref().unwrap().focused_item.is_some();
        let cursor_set = wave_loaded && self.waves.as_ref().unwrap().cursor.is_some();
        let multiple_viewports = wave_loaded && (self.waves.as_ref().unwrap().viewports.len() > 1);
        ui.with_layout(Layout::left_to_right(Align::LEFT), |ui| {
            if !self.show_menu() {
                // Menu
                ui.menu_button(RichText::new(icons::MENU_FILL).heading(), |ui| {
                    self.menu_contents(ui, msgs);
                });
                ui.separator();
            }
            // Files
            add_toolbar_button(
                ui,
                msgs,
                icons::FOLDER_OPEN_FILL,
                "Open file...",
                Message::OpenFileDialog(OpenMode::Open),
                true,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::DOWNLOAD_CLOUD_FILL,
                "Open URL...",
                Message::SetUrlEntryVisible(true),
                true,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::REFRESH_LINE,
                "Reload",
                Message::ReloadWaveform(self.config.behavior.keep_during_reload),
                wave_loaded,
            );
            ui.separator();

            // Zoom
            add_toolbar_button(
                ui,
                msgs,
                icons::ZOOM_IN_FILL,
                "Zoom in",
                Message::CanvasZoom {
                    mouse_ptr_timestamp: None,
                    delta: 0.5,
                    viewport_idx: 0,
                },
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::ZOOM_OUT_FILL,
                "Zoom out",
                Message::CanvasZoom {
                    mouse_ptr_timestamp: None,
                    delta: 2.0,
                    viewport_idx: 0,
                },
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                "⛶",
                "Zoom to fit",
                Message::ZoomToFit { viewport_idx: 0 },
                wave_loaded,
            );
            ui.separator();

            // Navigation
            add_toolbar_button(
                ui,
                msgs,
                icons::REWIND_START_FILL,
                "Go to start",
                Message::GoToStart { viewport_idx: 0 },
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::REWIND_FILL,
                "Go one page left",
                Message::CanvasScroll {
                    delta: Vec2 {
                        y: PER_SCROLL_EVENT * SCROLL_EVENTS_PER_PAGE,
                        x: 0.,
                    },
                    viewport_idx: 0,
                },
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::PLAY_REVERSE_FILL,
                "Go left",
                Message::CanvasScroll {
                    delta: Vec2 {
                        y: PER_SCROLL_EVENT,
                        x: 0.,
                    },
                    viewport_idx: 0,
                },
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::PLAY_FILL,
                "Go right",
                Message::CanvasScroll {
                    delta: Vec2 {
                        y: -PER_SCROLL_EVENT,
                        x: 0.,
                    },
                    viewport_idx: 0,
                },
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::SPEED_FILL,
                "Go one page right",
                Message::CanvasScroll {
                    delta: Vec2 {
                        y: -PER_SCROLL_EVENT * SCROLL_EVENTS_PER_PAGE,
                        x: 0.,
                    },
                    viewport_idx: 0,
                },
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::FORWARD_END_FILL,
                "Go to end",
                Message::GoToEnd { viewport_idx: 0 },
                wave_loaded,
            );
            ui.separator();

            // Next transition
            add_toolbar_button(
                ui,
                msgs,
                icons::CONTRACT_LEFT_FILL,
                "Set cursor on previous transition of focused signal",
                Message::MoveCursorToTransition {
                    next: false,
                    variable: None,
                },
                item_selected && cursor_set,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::CONTRACT_RIGHT_FILL,
                "Set cursor on next transition of focused signal",
                Message::MoveCursorToTransition {
                    next: true,
                    variable: None,
                },
                item_selected && cursor_set,
            );
            ui.separator();

            // Add items
            add_toolbar_button(
                ui,
                msgs,
                icons::SPACE,
                "Add divider",
                Message::AddDivider(None, None),
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::TIME_FILL,
                "Add timeline",
                Message::AddTimeLine(None),
                wave_loaded,
            );
            ui.separator();

            // Add items
            add_toolbar_button(
                ui,
                msgs,
                icons::ADD_BOX_FILL,
                "Add viewport",
                Message::AddViewport,
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                icons::CHECKBOX_INDETERMINATE_FILL,
                "Remove viewport",
                Message::RemoveViewport,
                wave_loaded && multiple_viewports,
            );
        });
    }
}
