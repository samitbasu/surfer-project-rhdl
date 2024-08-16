//! Help texts and dialogs.
use egui::{Context, Grid, OpenUrl, RichText, ScrollArea, Ui, Window};
use egui_remixicon::icons;
use emath::{Align2, Pos2};

use crate::wave_source::LoadOptions;
use crate::{message::Message, State};

impl State {
    pub fn help_message(&self, ui: &mut Ui) {
        if self.waves.is_none() {
            ui.label(RichText::new(
                "Drag and drop a VCD, FST, or GHW file here to open it",
            ));

            #[cfg(target_arch = "wasm32")]
            ui.label(RichText::new("Or press space and type load_url"));
            #[cfg(not(target_arch = "wasm32"))]
            ui.label(RichText::new(
                "Or press space and type load_file or load_url",
            ));
            #[cfg(target_arch = "wasm32")]
            ui.label(RichText::new(
                "Or use the file menu or toolbar to open a URL",
            ));
            #[cfg(not(target_arch = "wasm32"))]
            ui.label(RichText::new(
                "Or use the file menu or toolbar to open a file or a URL",
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
                if ui
                    .small_button(format!(
                        "Git version: {ver}",
                        ver = env!("VERGEN_GIT_DESCRIBE")
                    ))
                    .on_hover_text("Click to copy git version")
                    .clicked()
                {
                    ctx.output_mut(|o| o.copied_text = env!("VERGEN_GIT_DESCRIBE").to_string());
                }
                ui.label(format!(
                    "Build date: {date}",
                    date = env!("VERGEN_BUILD_DATE")
                ));
                ui.hyperlink_to(
                    (icons::GITLAB_FILL).to_string() + " repository",
                    "https://gitlab.com/surfer-project/surfer",
                );
                ui.hyperlink_to("Homepage", "https://surfer-project.org/");
                ui.add_space(10.);
                if ui.button("Close").clicked() {
                    msgs.push(Message::SetAboutVisible(false));
                }
            })
        });
    if !open {
        msgs.push(Message::SetAboutVisible(false));
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
                ui.label("- dragging a VCD, FST, or GHW file");
                #[cfg(target_arch = "wasm32")]
                ui.label("- typing load_url in the command palette");
                #[cfg(not(target_arch = "wasm32"))]
                ui.label("- typing load_url or load_file in the command palette");
                ui.label("- using the file menu");
                ui.label("- using the toolbar");
                ui.add_space(10.);
            });
            ui.vertical_centered(|ui| {
                if ui.button("Close").clicked() {
                    msgs.push(Message::SetQuickStartVisible(false));
                }
            })
        });
    if !open {
        msgs.push(Message::SetQuickStartVisible(false));
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
                    msgs.push(Message::SetKeyHelpVisible(false));
                }
            });
        });
    if !open {
        msgs.push(Message::SetKeyHelpVisible(false));
    }
}

fn key_listing(ui: &mut Ui) {
    let keys = vec![
        ("🚀", "Space", "Show command prompt"),
        ("↔", "Scroll", "Pan"),
        ("🔎", "Ctrl+Scroll", "Zoom"),
        (
            icons::LAYOUT_LEFT_FILL,
            "b",
            "Show or hide the design hierarchy",
        ),
        (icons::MENU_FILL, "m", "Show or hide menu"),
        (icons::TOOLS_FILL, "t", "Show or hide toolbar"),
        (icons::ZOOM_IN_FILL, "+", "Zoom in"),
        (icons::ZOOM_OUT_FILL, "-", "Zoom out"),
        ("", "k/⬆", "Scroll up"),
        ("", "j/⬇", "Scroll down"),
        ("", "Ctrl+k/⬆", "Move focused item up"),
        ("", "Ctrl+j/⬇", "Move focused item down"),
        ("", "Alt+k/⬆", "Move focus up"),
        ("", "Alt+j/⬇", "Move focus down"),
        ("", "a", "Add focused item to selection"),
        ("", "Ctrl+Alt+k/⬆", "Extend selection up"),
        ("", "Ctrl+Alt+j/⬇", "Extend selection down"),
        ("", "u/Shift+u", "Undo/redo last change"),
        ("", "Ctrl+z/Ctrl+y", "Undo/redo last change"),
        ("", "f", "Fast focus a signal"),
        ("", "Ctrl+0-9", "Add numbered marker"),
        ("", "0-9", "Center view at numbered marker"),
        (icons::REWIND_START_FILL, "s", "Go to start"),
        (icons::FORWARD_END_FILL, "e", "Go to end"),
        (icons::REFRESH_LINE, "r", "Reload waveform"),
        (icons::SPEED_FILL, "Page up", "Go one page/screen right"),
        (icons::REWIND_FILL, "Page down", "Go one page/screen left"),
        (
            icons::PLAY_FILL,
            "➡/l",
            "Go to next transition of focused variable (changeable in config)",
        ),
        (
            icons::PLAY_REVERSE_FILL,
            "⬅/h",
            "Go to previous transition of focused variable (changeable in config)",
        ),
        (
            "",
            "Ctrl+➡/l",
            "Go to next non-zero transition of focused variable",
        ),
        (
            "",
            "Ctrl+⬅/h",
            "Go to previous non-zero transition of focused variable",
        ),
        (icons::DELETE_BIN_2_FILL, "x/Delete", "Delete focused item"),
        #[cfg(not(target_arch = "wasm32"))]
        (icons::FULLSCREEN_LINE, "F11", "Toggle full screen"),
    ];

    Grid::new("keys")
        .num_columns(3)
        .spacing([5., 5.])
        .show(ui, |ui| {
            for (symbol, control, description) in keys {
                let control = ctrl_to_cmd(control);
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
        (
            icons::LAYOUT_LEFT_2_FILL,
            "b",
            "Show or hide the design hierarchy",
        ),
        (icons::MENU_FILL, "m", "Show or hide menu"),
        (icons::TOOLS_FILL, "t", "Show or hide toolbar"),
    ];

    Grid::new("controls")
        .num_columns(2)
        .spacing([20., 5.])
        .show(ui, |ui| {
            for (symbol, control, description) in controls {
                let control = ctrl_to_cmd(control);
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

pub fn draw_license_window(ctx: &Context, msgs: &mut Vec<Message>) {
    let mut open = true;
    let text = include_str!("../../LICENSE-EUPL-1.2.txt");
    Window::new("Surfer License")
        .open(&mut open)
        .collapsible(false)
        .max_height(600.)
        .default_width(600.)
        .show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.label(text);
            });
            ui.add_space(10.);
            ui.horizontal(|ui| {
                if ui.button("Dependency licenses").clicked() {
                    ctx.open_url(OpenUrl {
                        url: "https://docs.surfer-project.org/licenses.html".to_string(),
                        new_tab: true,
                    });
                }
                if ui.button("Close").clicked() {
                    msgs.push(Message::SetLicenseVisible(false));
                }
            });
        });
    if !open {
        msgs.push(Message::SetLicenseVisible(false));
    }
}

// Replace Ctrl with Cmd in case of macos, unless we are running tests
fn ctrl_to_cmd(instr: &str) -> String {
    #[cfg(all(target_os = "macos", not(test)))]
    let instring = instr.to_string().replace("Ctrl", "Cmd");
    #[cfg(any(not(target_os = "macos"), test))]
    let instring = instr.to_string();
    instring
}
