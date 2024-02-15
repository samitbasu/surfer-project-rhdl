use eframe::egui::{Button, Context, Layout, RichText, TopBottomPanel, Ui};
use eframe::emath::Align;
use eframe::epaint::Vec2;
use material_icons::{icon_to_char, Icon};

use crate::{
    message::Message,
    wave_data::{PER_SCROLL_EVENT, SCROLL_EVENTS_PER_PAGE},
    wave_source::OpenMode,
    State,
};

fn add_toolbar_button(
    ui: &mut Ui,
    msgs: &mut Vec<Message>,
    icon_string: String,
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

fn add_toolbar_button_with_icon(
    ui: &mut Ui,
    msgs: &mut Vec<Message>,
    icon: Icon,
    hover_text: &str,
    message: Message,
    enabled: bool,
) {
    add_toolbar_button(
        ui,
        msgs,
        icon_to_char(icon).to_string(),
        hover_text,
        message,
        enabled,
    );
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
        ui.with_layout(Layout::left_to_right(Align::LEFT), |ui| {
            if !self.show_menu() {
                // Menu
                ui.menu_button(RichText::new("☰").heading(), |ui| {
                    self.menu_contents(ui, msgs);
                });
                ui.separator();
            }
            // Files
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::FileOpen,
                "Open file...",
                Message::OpenFileDialog(OpenMode::Open),
                true,
            );
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::Download,
                "Open URL...",
                Message::SetUrlEntryVisible(true),
                true,
            );
            add_toolbar_button(
                ui,
                msgs,
                "⟳".to_string(),
                "Reload",
                Message::ReloadWaveform(self.config.behavior.keep_during_reload),
                wave_loaded,
            );
            ui.separator();

            // Zoom
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::ZoomIn,
                "Zoom in",
                Message::CanvasZoom {
                    mouse_ptr_timestamp: None,
                    delta: 0.5,
                },
                wave_loaded,
            );
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::ZoomOut,
                "Zoom out",
                Message::CanvasZoom {
                    mouse_ptr_timestamp: None,
                    delta: 2.0,
                },
                wave_loaded,
            );
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::FitScreen,
                "Zoom to fit",
                Message::ZoomToFit,
                wave_loaded,
            );
            ui.separator();

            // Navigation
            add_toolbar_button(
                ui,
                msgs,
                "⏮".to_string(),
                "Go to start",
                Message::GoToStart,
                wave_loaded,
            );
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::FastRewind,
                "Go one page left",
                Message::CanvasScroll {
                    delta: Vec2 {
                        y: PER_SCROLL_EVENT * SCROLL_EVENTS_PER_PAGE,
                        x: 0.,
                    },
                },
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                "⏴".to_string(),
                "Go left",
                Message::CanvasScroll {
                    delta: Vec2 {
                        y: PER_SCROLL_EVENT,
                        x: 0.,
                    },
                },
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                "⏵".to_string(),
                "Go right",
                Message::CanvasScroll {
                    delta: Vec2 {
                        y: -PER_SCROLL_EVENT,
                        x: 0.,
                    },
                },
                wave_loaded,
            );
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::FastForward,
                "Go one page right",
                Message::CanvasScroll {
                    delta: Vec2 {
                        y: -PER_SCROLL_EVENT * SCROLL_EVENTS_PER_PAGE,
                        x: 0.,
                    },
                },
                wave_loaded,
            );
            add_toolbar_button(
                ui,
                msgs,
                "⏭".to_string(),
                "Go to end",
                Message::GoToEnd,
                wave_loaded,
            );
            ui.separator();

            // Add items
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::SpaceBar,
                "Add divider",
                Message::AddDivider(None, None),
                wave_loaded,
            );
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::MoreTime,
                "Add timeline",
                Message::AddTimeLine(None),
                wave_loaded,
            );
            ui.separator();

            // Next transition
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::KeyboardTab,
                "Set cursor on previous transition of focused signal",
                Message::MoveCursorToTransition { next: false },
                item_selected,
            );
            add_toolbar_button_with_icon(
                ui,
                msgs,
                Icon::KeyboardTab,
                "Set cursor on next transition of focused signal",
                Message::MoveCursorToTransition { next: true },
                item_selected,
            );
        });
    }
}
