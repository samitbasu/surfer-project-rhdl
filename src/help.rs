use eframe::egui::{Context, Grid, RichText, Ui, Window};
use eframe::emath::{Align2, Pos2};

use crate::wave_source::LoadOptions;
use crate::{message::Message, State};

impl State {
    pub fn help_message(&self, ui: &mut Ui) {
        if self.waves.is_none() {
            ui.label(RichText::new(
                "Drag and drop a VCD or FST file here to open it",
            ));

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
                    self.sys
                        .channels
                        .msg_sender
                        .send(Message::LoadWaveformFileFromUrl(
                            "https://app.surfer-project.org/picorv32.vcd".to_string(),
                            LoadOptions::clean(),
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
        if self.show_wave_source {
            if let Some(waves) = &self.waves {
                ui.label(RichText::new(format!("Filename: {}", waves.source)).monospace());
            }
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

pub fn draw_about_window(ctx: &Context, msgs: &mut Vec<Message>) {
    let mut open = true;
    Window::new("About Surfer")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("🏄 Surfer").monospace().size(24.));
                ui.add_space(20.);
                ui.label(format!(
                    "Cargo version: {ver}",
                    ver = env!("CARGO_PKG_VERSION")
                ));
                ui.label(format!(
                    "Git version: {ver}",
                    ver = env!("VERGEN_GIT_DESCRIBE")
                ));
                ui.label(format!(
                    "Build date: {date}",
                    date = env!("VERGEN_BUILD_DATE")
                ));
                ui.hyperlink_to(" repository", "https://gitlab.com/surfer-project/surfer");
                ui.hyperlink_to("Homepage", "https://surfer-project.org/");
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

pub fn draw_quickstart_help_window(ctx: &Context, msgs: &mut Vec<Message>) {
    let mut open = true;
    Window::new("🏄 Surfer quick start")
        .collapsible(true)
        .resizable(true)
        .pivot(Align2::CENTER_CENTER)
        .open(&mut open)
        .default_pos(Pos2::new(
            ctx.available_rect().size().x / 2.,
            ctx.available_rect().size().y / 2.,
        ))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.add_space(5.);

                ui.label(RichText::new("Controls").size(20.));
                ui.add_space(5.);
                ui.label("↔ Use scroll and ctrl+scroll to navigate the waveform");
                ui.label("🚀 Press space to open the command palette");
                ui.label("✋ Click the middle mouse button for gestures");
                ui.label("❓ See the help menu for more controls");
                ui.add_space(10.);
                ui.label(RichText::new("Adding traces").size(20.));
                ui.add_space(5.);
                ui.label("Add more traces using the command palette or using the sidebar");
                ui.add_space(10.);
                ui.label(RichText::new("Opening files").size(20.));
                ui.add_space(5.);
                ui.label("Open a new file by");
                ui.label("- dragging a vcd file");
                #[cfg(target_arch = "wasm32")]
                ui.label("- typing load_url in the command palette");
                #[cfg(not(target_arch = "wasm32"))]
                ui.label("- typing load_url or load_vcd in the command palette");
                ui.label("- using the file menu");
                ui.add_space(10.);
            });
            ui.vertical_centered(|ui| {
                if ui.button("Close").clicked() {
                    msgs.push(Message::SetQuickStartVisible(false))
                }
            })
        });
    if !open {
        msgs.push(Message::SetQuickStartVisible(false))
    }
}

pub fn draw_control_help_window(ctx: &Context, msgs: &mut Vec<Message>) {
    let mut open = true;
    Window::new("🖮 Surfer controls")
        .collapsible(true)
        .resizable(true)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                key_listing(ui);
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

fn key_listing(ui: &mut Ui) {
    let keys = vec![
        ("🚀", "Space", "Show command prompt"),
        ("↔", "Scroll", "Pan"),
        ("🔎", "Ctrl+Scroll", "Zoom"),
        ("〰", "b", "Show or hide the design hierarchy"),
        ("☰", "m", "Show or hide menu"),
        ("🛠", "t", "Show or hide toolbar"),
        ("\u{e8ff}", "+", "Zoom in"),
        ("\u{e900}", "-", "Zoom out"),
        ("", "k/⬆", "Scroll up"),
        ("", "j/⬇", "Scroll down"),
        ("", "Ctrl+k/⬆", "Move focused item up"),
        ("", "Ctrl+j/⬇", "Move focused item down"),
        ("", "Alt+k/⬆", "Move focus up"),
        ("", "Alt+j/⬇", "Move focus down"),
        ("", "Ctrl+0-9", "Add numbered cursor"),
        ("", "0-9", "Center view at numbered cursor"),
        ("⏮", "s", "Go to start"),
        ("⏭", "e", "Go to end"),
        ("\u{e5d5}", "r", "Reload waveform"),
        ("\u{e01f}", "Page up", "Go one page/screen right"),
        ("\u{e020}", "Page down", "Go one page/screen left"),
        ("⏵", "➡", "Go right"),
        ("⏴", "⬅", "Go left"),
        ("🗙", "x/Delete", "Delete focused item"),
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

fn controls_listing(ui: &mut Ui) {
    let controls = vec![
        ("🚀", "Space", "Show command prompt"),
        ("↔", "Horizontal Scroll", "Pan"),
        ("↕", "j, k, Up, Down", "Scroll down/up"),
        ("⌖", "Ctrl+j, k, Up, Down", "Move focus down/up"),
        ("🔃", "Alt+j, k, Up, Down", "Move focused item down/up"),
        ("🔎", "Ctrl+Scroll", "Zoom"),
        ("〰", "b", "Show or hide the design hierarchy"),
        ("☰", "m", "Show or hide menu"),
        ("🛠", "t", "Show or hide toolbar"),
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

fn add_hint_text(ui: &mut Ui) {
    ui.add_space(20.);
    ui.label(RichText::new("Hint: You can repeat keybinds by typing Alt+0-9 before them. For example, Alt+1 Alt+0 k scrolls 10 steps up."));
}
