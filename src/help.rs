use eframe::{
    egui::{self, Grid, RichText},
    epaint::Vec2,
};

use crate::{message::Message, State};

impl State {
    pub fn help_message(&self, ui: &mut egui::Ui) {
        if self.waves.is_none() {
            ui.label(RichText::new("Drag and drop a VCD file here to open it"));

            #[cfg(target_arch = "wasm32")]
            ui.label(RichText::new("Or press space and type load_url"));
            #[cfg(not(target_arch = "wasm32"))]
            ui.label(RichText::new(
                "Or press space and type load_vcd or load_url",
            ));
            #[cfg(target_arch = "wasm32")]
            ui.label(RichText::new("Or use the file menu to open a URL"));
            #[cfg(not(target_arch = "wasm32"))]
            ui.label(RichText::new(
                "Or use the file menu to open a file or a URL",
            ));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Or click"));
                if ui.link("here").clicked() {
                    self.msg_sender
                        .send(Message::LoadVcdFromUrl(
                            "https://app.surfer-project.org/picorv32.vcd".to_string(),
                        ))
                        .ok();
                }
                ui.label("to open an example waveform");
            });

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(20.0);
        }

        controls_listing(ui);

        ui.add_space(20.0);
        ui.separator();
        ui.add_space(20.0);
        if let Some(waves) = &self.waves {
            ui.label(RichText::new(format!("Filename: {}", waves.source)).monospace());
        }

        #[cfg(target_arch = "wasm32")]
        {
            ui.label(RichText::new(
            "Note that this web based version is a bit slower than a natively installed version. There may also be a long delay with unresponsiveness when loading large waveforms because the web assembly version does not currently support multi threading.",
        ));

            ui.hyperlink_to(
                "See https://gitlab.com/surfer-project/surfer for install instructions",
                "https://gitlab.com/surfer-project/surfer",
            );
        }
    }
}

pub fn draw_about_window(ctx: &egui::Context, msgs: &mut Vec<Message>) {
    let mut open = true;
    egui::Window::new("About Surfer")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("🏄 Surfer").monospace().size(24.));
                ui.add_space(20.);
                ui.label(format!("Version: {ver}", ver = env!("CARGO_PKG_VERSION")));
                ui.label(format!(
                    "Exact version: {info}",
                    info = env!("VERGEN_GIT_DESCRIBE")
                ));
                ui.label(format!(
                    "Build date: {date}",
                    date = env!("VERGEN_BUILD_DATE")
                ));
                ui.hyperlink_to(" repository", "https://gitlab.com/surfer-project/surfer");
                ui.add_space(10.);
                if ui.button("Close").clicked() {
                    msgs.push(Message::SetAboutVisible(false))
                }
            });
        });
    if !open {
        msgs.push(Message::SetAboutVisible(false))
    }
}

pub fn draw_control_help_window(
    ctx: &egui::Context,
    max_width: f32,
    max_height: f32,
    msgs: &mut Vec<Message>,
) {
    let mut open = true;
    egui::Window::new("🖮 Surfer control")
        .collapsible(true)
        .resizable(true)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let layout = egui::Layout::top_down(egui::Align::LEFT);
                ui.allocate_ui_with_layout(
                    Vec2 {
                        x: max_width * 0.35,
                        y: max_height * 0.5,
                    },
                    layout,
                    |ui| key_listing(ui),
                );
                ui.add_space(10.);
                if ui.button("Close").clicked() {
                    msgs.push(Message::SetKeyHelpVisible(false))
                }
            });
        });
    if !open {
        msgs.push(Message::SetKeyHelpVisible(false))
    }
}

fn key_listing(ui: &mut egui::Ui) {
    let keys = vec![
        ("🚀", "Space", "Show command prompt"),
        ("↔", "Scroll", "Pan"),
        ("🔎", "Ctrl+Scroll", "Zoom"),
        ("〰", "b", "Show or hide the design hierarchy"),
        ("☰", "m", "Show or hide menu"),
        ("🔎➕", "+", "Zoom in"),
        ("🔎➖", "-", "Zoom out"),
        ("", "k/⬆", "Scroll up"),
        ("", "j/⬇", "Scroll down"),
        ("", "Ctrl+k/⬆", "Move focused item up"),
        ("", "Ctrl+j/⬇", "Move focused item down"),
        ("", "Alt+k/⬆", "Move focus up"),
        ("", "Alt+j/⬇", "Move focus down"),
        ("", "Ctrl+0-9", "Add numbered cursor"),
        ("", "0-9", "Center view at numbered cursor"),
        ("🔙", "s", "Scroll to start"),
        ("🔚", "e", "Scroll to end"),
        ("🗙", "Delete", "Delete focused item"),
        #[cfg(not(target_arch = "wasm32"))]
        ("⛶", "F11", "Toggle full screen"),
    ];

    Grid::new("keys")
        .num_columns(3)
        .spacing([5., 5.])
        .show(ui, |ui| {
            for (symbol, control, description) in keys {
                ui.label(symbol);
                ui.label(control);
                ui.label(description);
                ui.end_row();
            }
        });

    add_hint_text(ui);
}

fn controls_listing(ui: &mut egui::Ui) {
    let controls = vec![
        ("🚀", "Space", "Show command prompt"),
        ("↔", "Horizontal Scroll", "Pan"),
        ("↕", "j, k, Up, Down", "Scroll down/up"),
        ("⌖", "Ctrl+j, k, Up, Down", "Move focus down/up"),
        ("🔃", "Alt+j, k, Up, Down", "Move focused item down/up"),
        ("🔎", "Ctrl+Scroll", "Zoom"),
        ("〰", "b", "Show or hide the design hierarchy"),
        ("☰", "m", "Show or hide menu"),
    ];

    Grid::new("controls")
        .num_columns(2)
        .spacing([20., 5.])
        .show(ui, |ui| {
            for (symbol, control, description) in controls {
                ui.label(format!("{symbol}  {control}"));
                ui.label(description);
                ui.end_row();
            }
        });
    add_hint_text(ui);
}

fn add_hint_text(ui: &mut egui::Ui) {
    ui.add_space(20.);
    ui.label(RichText::new("Hint: You can repeat keybinds by typing Alt+0-9 before them. For example, Alt+1 Alt+0 k scrolls 10 steps up."));
}
