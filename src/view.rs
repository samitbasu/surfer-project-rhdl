use color_eyre::eyre::Context;
use eframe::egui::{self, style::Margin, Align, Color32, Event, Key, Layout, Painter, RichText};
use eframe::egui::{menu, Frame, Grid, Sense, TextStyle, Ui};
use eframe::emath::RectTransform;
use eframe::epaint::{Pos2, Rect, Rounding, Vec2};
use fastwave_backend::{Metadata, SignalIdx, Timescale};
use itertools::Itertools;
use log::{info, warn};
use num::{BigInt, BigRational, ToPrimitive};
use regex::Regex;
use spade_common::num_ext::InfallibleToBigInt;

use crate::config::SurferTheme;
use crate::util::uint_idx_to_alpha_idx;
use crate::wave_container::{FieldRef, ModuleRef, SignalRef};
use crate::{
    command_prompt::show_command_prompt,
    translation::{SignalInfo, TranslationPreference},
    Message, MoveDir, State, WaveData,
};
use crate::{ClockHighlightType, DisplayedItem, LoadProgress, SignalFilterType, SignalNameType};

pub struct DrawingContext<'a> {
    pub painter: &'a mut Painter,
    pub cfg: &'a DrawConfig,
    pub to_screen: &'a dyn Fn(f32, f32) -> Pos2,
    pub theme: &'a SurferTheme,
}

#[derive(Debug)]
pub struct DrawConfig {
    pub canvas_height: f32,
    pub line_height: f32,
    pub max_transition_width: i32,
}

#[derive(Debug)]
pub struct SignalDrawingInfo {
    pub field_ref: FieldRef,
    pub signal_list_idx: usize,
    pub offset: f32,
}

#[derive(Debug)]
pub struct DividerDrawingInfo {
    pub signal_list_idx: usize,
    pub offset: f32,
}

#[derive(Debug)]
pub struct CursorDrawingInfo {
    pub signal_list_idx: usize,
    pub offset: f32,
    pub idx: u8,
}

pub enum ItemDrawingInfo {
    Signal(SignalDrawingInfo),
    Divider(DividerDrawingInfo),
    Cursor(CursorDrawingInfo),
}

impl ItemDrawingInfo {
    pub fn offset(&self) -> f32 {
        match self {
            ItemDrawingInfo::Signal(drawing_info) => drawing_info.offset,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.offset,
            ItemDrawingInfo::Cursor(drawing_info) => drawing_info.offset,
        }
    }
    pub fn signal_list_idx(&self) -> usize {
        match self {
            ItemDrawingInfo::Signal(drawing_info) => drawing_info.signal_list_idx,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.signal_list_idx,
            ItemDrawingInfo::Cursor(drawing_info) => drawing_info.signal_list_idx,
        }
    }
}

impl eframe::App for State {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(not(target_arch = "wasm32"))]
        let window_size = Some(frame.info().window_info.size);
        #[cfg(target_arch = "wasm32")]
        let window_size = None;

        let mut msgs = self.draw(ctx, window_size);

        while let Some(msg) = msgs.pop() {
            #[cfg(not(target_arch = "wasm32"))]
            if let Message::Exit = msg {
                frame.close()
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Message::ToggleFullscreen = msg {
                frame.set_fullscreen(!frame.info().window_info.fullscreen)
            }
            self.update(msg);
        }

        self.handle_async_messages();
    }
}

impl State {
    pub(crate) fn draw(&self, ctx: &egui::Context, window_size: Option<Vec2>) -> Vec<Message> {
        let max_width = ctx.available_rect().width();
        let max_height = ctx.available_rect().height();

        let mut msgs = vec![];
        if self.config.layout.show_menu {
            egui::TopBottomPanel::top("menu").show(ctx, |ui| {
                self.draw_menu(ui, &mut msgs);
            });
        }
        if let Some(vcd) = &self.waves {
            egui::TopBottomPanel::bottom("modeline")
                .frame(egui::containers::Frame {
                    fill: self.config.theme.primary_ui_color.background,
                    ..Default::default()
                })
                .show(ctx, |ui| {
                    ui.visuals_mut().override_text_color =
                        Some(self.config.theme.primary_ui_color.foreground);
                    ui.with_layout(Layout::left_to_right(Align::RIGHT), |ui| {
                        ui.add_space(10.0);
                        if self.show_wave_source {
                            ui.label(&vcd.source.to_string());
                            if let Some(datetime) = vcd.inner.metadata().date {
                                ui.add_space(10.0);
                                ui.label(format!("Generated: {datetime}"));
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::RIGHT), |ui| {
                            if let Some(time) = &vcd.cursor {
                                ui.label(time_string(
                                    time,
                                    &vcd.inner.metadata(),
                                    &self.wanted_timescale,
                                ))
                                .context_menu(|ui| {
                                    timescale_menu(ui, &mut msgs, &self.wanted_timescale)
                                });
                                ui.add_space(10.0)
                            }
                            if let Some(count) = &self.count {
                                ui.label(format!("Count: {}", count));
                                ui.add_space(20.0);
                            }
                        });
                    });
                });
        }

        if self.config.layout.show_hierarchy {
            egui::SidePanel::left("signal select left panel")
                .default_width(300.)
                .width_range(100.0..=max_width)
                .frame(egui::containers::Frame {
                    fill: self.config.theme.primary_ui_color.background,
                    ..Default::default()
                })
                .show(ctx, |ui| {
                    ui.visuals_mut().override_text_color =
                        Some(self.config.theme.primary_ui_color.foreground);
                    ui.with_layout(
                        Layout::top_down(Align::LEFT).with_cross_justify(true),
                        |ui| {
                            let total_space = ui.available_height();
                            egui::Frame::none()
                                .inner_margin(Margin::same(5.0))
                                .show(ui, |ui| {
                                    ui.set_max_height(total_space / 2.);
                                    ui.set_min_height(total_space / 2.);

                                    ui.heading("Modules");
                                    ui.add_space(3.0);

                                    egui::ScrollArea::both()
                                        .id_source("modules")
                                        .show(ui, |ui| {
                                            ui.style_mut().wrap = Some(false);
                                            if let Some(vcd) = &self.waves {
                                                self.draw_all_scopes(&mut msgs, vcd, ui);
                                            }
                                        });
                                });

                            egui::Frame::none()
                                .inner_margin(Margin::same(5.0))
                                .show(ui, |ui| {
                                    let filter = &mut *self.signal_filter.borrow_mut();
                                    ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
                                        ui.heading("Signals");
                                        ui.add_space(3.0);
                                        ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                                            ui.button("➕")
                                                .on_hover_text("Add all signals")
                                                .clicked()
                                                .then(|| {
                                                    if let Some(waves) = self.waves.as_ref() {
                                                        // Iterate over the reversed list to get
                                                        // waves in the same order as the signal
                                                        // list
                                                        for sig in self
                                                            .listed_signals(waves, filter)
                                                            .into_iter()
                                                            .rev()
                                                        {
                                                            msgs.push(Message::AddSignal(sig))
                                                        }
                                                    }
                                                });
                                            ui.button("❌")
                                                .on_hover_text("Clear filter")
                                                .clicked()
                                                .then(|| filter.clear());

                                            // Check if regex and if an incorrect regex, change background color
                                            if self.signal_filter_type == SignalFilterType::Regex
                                                && Regex::new(filter).is_err()
                                            {
                                                ui.style_mut().visuals.extreme_bg_color =
                                                    self.config.theme.accent_error.background;
                                            }
                                            // Create text edit
                                            let response = ui
                                                .add(
                                                    egui::TextEdit::singleline(filter).hint_text(
                                                        "Filter (context menu for type)",
                                                    ),
                                                )
                                                .context_menu(|ui| {
                                                    signal_filter_type_menu(
                                                        ui,
                                                        &mut msgs,
                                                        &self.signal_filter_type,
                                                    )
                                                });
                                            // Handle focus
                                            if response.gained_focus() {
                                                msgs.push(Message::SetFilterFocused(true));
                                            }
                                            if response.lost_focus() {
                                                msgs.push(Message::SetFilterFocused(false));
                                            }
                                        })
                                    });
                                    ui.add_space(3.0);

                                    egui::ScrollArea::both()
                                        .id_source("signals")
                                        .show(ui, |ui| {
                                            if let Some(vcd) = &self.waves {
                                                self.draw_signal_list(&mut msgs, vcd, ui, filter);
                                            }
                                        });
                                });
                        },
                    )
                });
        }

        if self.command_prompt.visible {
            show_command_prompt(self, ctx, window_size, &mut msgs);
        }

        if let Some(vcd_progress_data) = &self.vcd_progress {
            egui::TopBottomPanel::top("progress panel").show(ctx, |ui| {
                ui.vertical_centered_justified(|ui| match vcd_progress_data {
                    LoadProgress::Downloading(url) => {
                        ui.spinner();
                        ui.monospace(format!("Downloading {url}"));
                    }
                    LoadProgress::Loading(total_bytes, bytes_done) => {
                        let num_bytes = bytes_done.load(std::sync::atomic::Ordering::Relaxed);

                        if let Some(total) = total_bytes {
                            ui.monospace(format!("Loading. {num_bytes}/{total} kb loaded"));
                            let progress = num_bytes as f32 / *total as f32;
                            let progress_bar = egui::ProgressBar::new(progress)
                                .show_percentage()
                                .desired_width(300.);

                            ui.add(progress_bar);
                        } else {
                            ui.spinner();
                            ui.monospace(format!("Loading. {num_bytes} bytes loaded"));
                        };
                    }
                });
            });
        }

        if let Some(vcd) = &self.waves {
            if !vcd.displayed_items.is_empty() {
                let item_offsets = egui::SidePanel::left("signal list")
                    .default_width(200.)
                    .width_range(100.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap = Some(false);

                        if ui.ui_contains_pointer() {
                            let scroll_delta = ui.input(|i| i.scroll_delta);
                            if scroll_delta.y > 0.0 {
                                msgs.push(Message::InvalidateCount);
                                msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
                            } else if scroll_delta.y < 0.0 {
                                msgs.push(Message::InvalidateCount);
                                msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
                            }
                        }

                        ui.with_layout(
                            Layout::top_down(Align::LEFT).with_cross_justify(true),
                            |ui| self.draw_item_list(&mut msgs, &vcd, ui),
                        )
                        .inner
                    })
                    .inner;

                egui::SidePanel::left("signal values")
                    .default_width(100.)
                    .width_range(30.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap = Some(false);
                        if ui.ui_contains_pointer() {
                            let scroll_delta = ui.input(|i| i.scroll_delta);
                            if scroll_delta.y > 0.0 {
                                msgs.push(Message::InvalidateCount);
                                msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
                            } else if scroll_delta.y < 0.0 {
                                msgs.push(Message::InvalidateCount);
                                msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
                            }
                        }
                        ui.with_layout(
                            Layout::top_down(Align::LEFT).with_cross_justify(true),
                            |ui| {
                                egui::ScrollArea::horizontal().show(ui, |ui| {
                                    self.draw_var_values(&item_offsets, vcd, ui, &mut msgs)
                                })
                            },
                        )
                    });

                egui::CentralPanel::default()
                    .frame(Frame {
                        inner_margin: Margin::same(0.0),
                        outer_margin: Margin::same(0.0),
                        ..Default::default()
                    })
                    .show(ctx, |ui| {
                        self.draw_signals(&mut msgs, &item_offsets, ui);
                    });

                if let Some(idx) = self.rename_target {
                    let mut open = true;
                    let name = &mut *self.item_renaming_string.borrow_mut();
                    egui::Window::new("Rename item")
                        .open(&mut open)
                        .collapsible(false)
                        .resizable(true)
                        .show(ctx, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.text_edit_singleline(name);
                                ui.horizontal(|ui| {
                                    if ui.button("Rename").clicked() {
                                        msgs.push(Message::ItemNameChange(Some(idx), name.clone()));
                                        msgs.push(Message::SetRenameItemVisible(false))
                                    }
                                    if ui.button("Cancel").clicked() {
                                        msgs.push(Message::SetRenameItemVisible(false))
                                    }
                                });
                            });
                        });
                    if !open {
                        msgs.push(Message::SetRenameItemVisible(false))
                    }
                }
            }
        };

        if self.waves.is_none()
            || self
                .waves
                .as_ref()
                .map_or(false, |vcd| vcd.displayed_items.is_empty())
        {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(self.config.theme.canvas_colors.background))
                .show(ctx, |ui| {
                    ui.add_space(max_height * 0.3);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🏄 Surfer").monospace().size(24.));
                        ui.add_space(20.);
                        let layout = egui::Layout::top_down(egui::Align::LEFT);
                        ui.allocate_ui_with_layout(
                            Vec2 {
                                x: max_width * 0.35,
                                y: max_height * 0.5,
                            },
                            layout,
                            |ui| self.help_message(ui),
                        );
                    });
                });
        }

        if self.show_about {
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

        if self.show_keys {
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
                            |ui| self.key_listing(ui),
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

        if self.show_gestures {
            self.mouse_gesture_help(ctx, &mut msgs);
        }

        if self.show_url_entry {
            let mut open = true;
            egui::Window::new("Load URL")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        let url = &mut *self.url.borrow_mut();
                        let response = ui.text_edit_singleline(url);
                        ui.horizontal(|ui| {
                            if ui.button("Load URL").clicked()
                                || (response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                            {
                                msgs.push(Message::LoadVcdFromUrl(url.clone()));
                                msgs.push(Message::SetUrlEntryVisible(false))
                            }
                            if ui.button("Cancel").clicked() {
                                msgs.push(Message::SetUrlEntryVisible(false))
                            }
                        });
                    });
                });
            if !open {
                msgs.push(Message::SetUrlEntryVisible(false))
            }
        }

        ctx.input(|i| {
            i.raw.dropped_files.iter().for_each(|file| {
                info!("Got dropped file");
                msgs.push(Message::FileDropped(file.clone()))
            })
        });

        // If some dialogs are open, skip decoding keypresses
        if !self.show_url_entry && self.rename_target.is_none() {
            self.handle_pressed_keys(ctx, &mut msgs);
        }

        msgs
    }

    fn handle_pressed_keys(&self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        ctx.input(|i| {
            i.events.iter().for_each(|event| match event {
                Event::Key {
                    key,
                    repeat: _,
                    pressed,
                    modifiers,
                } => match (
                    key,
                    pressed,
                    self.command_prompt.visible,
                    self.signal_filter_focused,
                ) {
                    (Key::Num0, true, false, false) => {
                        handle_digit(0, modifiers, msgs);
                    }
                    (Key::Num1, true, false, false) => {
                        handle_digit(1, modifiers, msgs);
                    }
                    (Key::Num2, true, false, false) => {
                        handle_digit(2, modifiers, msgs);
                    }
                    (Key::Num3, true, false, false) => {
                        handle_digit(3, modifiers, msgs);
                    }
                    (Key::Num4, true, false, false) => {
                        handle_digit(4, modifiers, msgs);
                    }
                    (Key::Num5, true, false, false) => {
                        handle_digit(5, modifiers, msgs);
                    }
                    (Key::Num6, true, false, false) => {
                        handle_digit(6, modifiers, msgs);
                    }
                    (Key::Num7, true, false, false) => {
                        handle_digit(7, modifiers, msgs);
                    }
                    (Key::Num8, true, false, false) => {
                        handle_digit(8, modifiers, msgs);
                    }
                    (Key::Num9, true, false, false) => {
                        handle_digit(9, modifiers, msgs);
                    }
                    (Key::Home, true, false, false) => msgs.push(Message::SetVerticalScroll(0)),
                    (Key::End, true, false, false) => {
                        if let Some(vcd) = &self.waves {
                            if vcd.displayed_items.len() > 1 {
                                msgs.push(Message::SetVerticalScroll(
                                    vcd.displayed_items.len() - 1,
                                ));
                            }
                        }
                    }
                    (Key::Space, true, false, false) => msgs.push(Message::ShowCommandPrompt(true)),
                    (Key::Escape, true, true, false) => {
                        msgs.push(Message::ShowCommandPrompt(false))
                    }
                    (Key::Escape, true, false, false) => msgs.push(Message::InvalidateCount),
                    (Key::Escape, true, _, true) => msgs.push(Message::SetFilterFocused(false)),
                    (Key::B, true, false, false) => msgs.push(Message::ToggleSidePanel),
                    (Key::M, true, false, false) => msgs.push(Message::ToggleMenu),
                    (Key::F11, true, false, _) => msgs.push(Message::ToggleFullscreen),
                    (Key::S, true, false, false) => msgs.push(Message::GoToStart),
                    (Key::E, true, false, false) => msgs.push(Message::GoToEnd),
                    (Key::Minus, true, false, false) => msgs.push(Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 2.0,
                    }),
                    (Key::PlusEquals, true, false, false) => msgs.push(Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 0.5,
                    }),
                    (Key::J, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(MoveDir::Down, self.get_count()));
                        } else if modifiers.ctrl {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Down, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::K, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(MoveDir::Up, self.get_count()));
                        } else if modifiers.ctrl {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Up, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::ArrowDown, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(MoveDir::Down, self.get_count()));
                        } else if modifiers.ctrl {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Down, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::ArrowUp, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(MoveDir::Up, self.get_count()));
                        } else if modifiers.ctrl {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Up, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::Delete, true, false, false) => {
                        if let Some(vcd) = &self.waves {
                            if let Some(idx) = vcd.focused_item {
                                msgs.push(Message::RemoveItem(idx, self.get_count()));
                                msgs.push(Message::InvalidateCount);
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            })
        });
    }

    fn listed_signals(&self, waves: &WaveData, filter: &str) -> Vec<SignalRef> {
        if let Some(scope) = &waves.active_module {
            let listed = waves
                .inner
                .signals_in_module(scope)
                .iter()
                .filter(|sig| self.signal_filter_type.is_match(&sig.name, filter))
                .sorted_by(|a, b| human_sort::compare(&a.name, &b.name))
                .cloned()
                .collect_vec();

            listed
        } else {
            vec![]
        }
    }
}

fn handle_digit(digit: u8, modifiers: &egui::Modifiers, msgs: &mut Vec<Message>) {
    if modifiers.alt {
        msgs.push(Message::AddCount((digit + 48) as char))
    } else if modifiers.ctrl {
        msgs.push(Message::SetCursorPosition(digit))
    } else {
        msgs.push(Message::GoToCursorPosition(digit))
    }
}

impl State {
    pub fn draw_all_scopes(&self, msgs: &mut Vec<Message>, wave: &WaveData, ui: &mut egui::Ui) {
        for module in wave.inner.root_modules() {
            self.draw_selectable_child_or_orphan_scope(msgs, wave, &module, ui);
        }
    }

    fn draw_selectable_child_or_orphan_scope(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        module: &ModuleRef,
        ui: &mut egui::Ui,
    ) {
        let name = module.name();

        let Some(child_modules) = wave
            .inner
            .child_modules(module)
            .context("Faield to get child modules")
            .map_err(|e| warn!("{e:#?}"))
            .ok()
        else {
            return;
        };

        if child_modules.is_empty() {
            ui.add(egui::SelectableLabel::new(
                wave.active_module == Some(module.clone()),
                name,
            ))
            .clicked()
            .then(|| msgs.push(Message::SetActiveScope(module.clone())));
        } else {
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                egui::Id::new(module),
                false,
            )
            .show_header(ui, |ui| {
                ui.with_layout(
                    Layout::top_down(Align::LEFT).with_cross_justify(true),
                    |ui| {
                        ui.add(egui::SelectableLabel::new(
                            wave.active_module == Some(module.clone()),
                            name,
                        ))
                        .clicked()
                        .then(|| msgs.push(Message::SetActiveScope(module.clone())))
                    },
                );
            })
            .body(|ui| self.draw_root_scope_view(msgs, wave, module, ui));
        }
    }

    fn draw_root_scope_view(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        root_module: &ModuleRef,
        ui: &mut egui::Ui,
    ) {
        let Some(child_modules) = wave
            .inner
            .child_modules(root_module)
            .context("Faield to get child modules")
            .map_err(|e| warn!("{e:#?}"))
            .ok()
        else {
            return;
        };

        for child_module in child_modules {
            self.draw_selectable_child_or_orphan_scope(msgs, wave, &child_module, ui);
        }
    }

    fn draw_signal_list(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        ui: &mut egui::Ui,
        filter: &str,
    ) {
        for sig in self.listed_signals(wave, filter) {
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    ui.add(egui::SelectableLabel::new(false, sig.name.clone()))
                        .clicked()
                        .then(|| msgs.push(Message::AddSignal(sig.clone())));
                },
            );
        }
    }

    fn draw_item_list(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &WaveData,
        ui: &mut egui::Ui,
    ) -> Vec<ItemDrawingInfo> {
        let mut item_offsets = Vec::new();

        for (vidx, displayed_item) in vcd.displayed_items.iter().enumerate().skip(vcd.scroll) {
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| match displayed_item {
                    DisplayedItem::Signal(displayed_signal) => {
                        let sig = displayed_signal;
                        let info = &displayed_signal.info;

                        self.draw_signal_var(
                            msgs,
                            vidx,
                            &displayed_signal.display_name,
                            FieldRef {
                                root: sig.signal_ref.clone(),
                                field: vec![],
                            },
                            &mut item_offsets,
                            info,
                            ui,
                        );
                    }
                    DisplayedItem::Divider(_) => {
                        self.draw_plain_var(msgs, vidx, displayed_item, &mut item_offsets, ui);
                    }
                    DisplayedItem::Cursor(_) => {
                        self.draw_plain_var(msgs, vidx, displayed_item, &mut item_offsets, ui);
                    }
                },
            );
        }

        item_offsets
    }

    fn draw_signal_var(
        &self,
        msgs: &mut Vec<Message>,
        vidx: usize,
        name: &str,
        field: FieldRef,
        item_offsets: &mut Vec<ItemDrawingInfo>,
        info: &SignalInfo,
        ui: &mut egui::Ui,
    ) {
        let mut draw_label = |ui: &mut egui::Ui| {
            let tooltip = if let Some(waves) = &self.waves {
                if field.field.len() == 0 {
                    format!(
                        "{}\nNum bits: {}",
                        field.root.full_path_string(),
                        waves
                            .inner
                            .signal_meta(&field.root)
                            .ok()
                            .and_then(|meta| meta.num_bits)
                            .map(|num_bits| format!("{num_bits}"))
                            .unwrap_or("unknown".to_string())
                    )
                } else {
                    "From translator".to_string()
                }
            } else {
                "No VCD loaded".to_string()
            };

            ui.horizontal_top(|ui| {
                if self.command_prompt.expanded.starts_with("signal_focus") {
                    self.add_alpha_id(vidx, ui);
                }

                self.add_focus_marker(vidx, ui);

                let signal_label = ui
                    .selectable_label(false, egui::RichText::new(name))
                    .on_hover_text(tooltip)
                    .context_menu(|ui| {
                        self.item_context_menu(Some(&field), msgs, ui, vidx);
                    });
                if signal_label.clicked() {
                    msgs.push(Message::FocusItem(vidx))
                }
                signal_label
            })
        };

        match info {
            SignalInfo::Compound { subfields } => {
                let response = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    egui::Id::new(&field),
                    false,
                )
                .show_header(ui, draw_label)
                .body(|ui| {
                    for (name, info) in subfields {
                        let mut new_path = field.clone();
                        new_path.field.push(name.clone());
                        self.draw_signal_var(msgs, vidx, name, new_path, item_offsets, info, ui);
                    }
                });

                let offset = response.0.rect.top();
                item_offsets.push(ItemDrawingInfo::Signal(SignalDrawingInfo {
                    field_ref: field.clone(),
                    signal_list_idx: vidx,
                    offset,
                }));
            }
            SignalInfo::Bool
            | SignalInfo::Bits
            | SignalInfo::Clock
            | SignalInfo::String
            | SignalInfo::Real => {
                let label = draw_label(ui);
                item_offsets.push(ItemDrawingInfo::Signal(SignalDrawingInfo {
                    field_ref: field.clone(),
                    signal_list_idx: vidx,
                    offset: label.inner.rect.top(),
                }));
            }
        }
    }

    fn add_focus_marker(&self, vidx: usize, ui: &mut egui::Ui) {
        let focus_marker_color = if self
            .waves
            .as_ref()
            .expect("Can't draw a signal without a loaded waveform.")
            .focused_item
            .map(|focused| focused == vidx)
            .unwrap_or(false)
        {
            self.config.theme.accent_info.background
        } else {
            Color32::TRANSPARENT
        };
        ui.colored_label(focus_marker_color, "♦");
    }

    fn draw_plain_var(
        &self,
        msgs: &mut Vec<Message>,
        vidx: usize,
        displayed_item: &DisplayedItem,
        item_offsets: &mut Vec<ItemDrawingInfo>,
        ui: &mut egui::Ui,
    ) {
        let mut draw_label = |ui: &mut egui::Ui| {
            ui.horizontal_top(|ui| {
                if self.command_prompt.expanded.starts_with("focus") {
                    self.add_alpha_id(vidx, ui);
                }

                self.add_focus_marker(vidx, ui);

                let text_color = if let Some(color) = &displayed_item.color() {
                    self.config
                        .theme
                        .colors
                        .get(color)
                        .unwrap_or(&self.config.theme.foreground)
                } else {
                    &self.config.theme.foreground
                };

                let signal_label = ui
                    .selectable_label(
                        false,
                        egui::RichText::new(displayed_item.display_name().clone())
                            .color(*text_color),
                    )
                    .context_menu(|ui| {
                        self.item_context_menu(None, msgs, ui, vidx);
                    });
                if signal_label.clicked() {
                    msgs.push(Message::FocusItem(vidx))
                }
                signal_label
            })
        };

        let label = draw_label(ui);
        match displayed_item {
            DisplayedItem::Divider(_) => {
                item_offsets.push(ItemDrawingInfo::Divider(DividerDrawingInfo {
                    signal_list_idx: vidx,
                    offset: label.inner.rect.top(),
                }))
            }
            DisplayedItem::Cursor(cursor) => {
                item_offsets.push(ItemDrawingInfo::Cursor(CursorDrawingInfo {
                    signal_list_idx: vidx,
                    offset: label.inner.rect.top(),
                    idx: cursor.idx,
                }))
            }
            &DisplayedItem::Signal(_) => {}
        }
    }

    fn add_alpha_id(&self, vidx: usize, ui: &mut egui::Ui) {
        let alpha_id = uint_idx_to_alpha_idx(
            vidx,
            self.waves
                .as_ref()
                .map_or(0, |vcd| vcd.displayed_items.len()),
        );
        ui.label(
            egui::RichText::new(alpha_id)
                .background_color(self.config.theme.accent_warn.background)
                .monospace()
                .color(self.config.theme.accent_warn.foreground),
        );
    }

    fn item_context_menu(
        &self,
        path: Option<&FieldRef>,
        msgs: &mut Vec<Message>,
        ui: &mut egui::Ui,
        vidx: usize,
    ) {
        if let Some(path) = path {
            self.add_format_menu(path, msgs, ui);
        }

        let displayed_item = &self.waves.as_ref().unwrap().displayed_items[vidx];
        ui.menu_button("Color", |ui| {
            let selected_color = &displayed_item
                .color()
                .clone()
                .unwrap_or("__nocolor__".to_string());
            for color_name in self.config.theme.colors.keys() {
                ui.radio(selected_color == color_name, color_name)
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::ItemColorChange(
                            Some(vidx),
                            Some(color_name.clone()),
                        ));
                    });
            }
            ui.separator();
            ui.radio(selected_color == "__nocolor__", "Default")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ItemColorChange(Some(vidx), None));
                });
        });

        ui.menu_button("Background color", |ui| {
            let selected_color = &displayed_item
                .background_color()
                .clone()
                .unwrap_or("__nocolor__".to_string());
            for color_name in self.config.theme.colors.keys() {
                ui.radio(selected_color == color_name, color_name)
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::ItemBackgroundColorChange(
                            Some(vidx),
                            Some(color_name.clone()),
                        ));
                    });
            }
            ui.separator();
            ui.radio(selected_color == "__nocolor__", "Default")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ItemBackgroundColorChange(Some(vidx), None));
                });
        });

        if let DisplayedItem::Signal(signal) = &self.waves.as_ref().unwrap().displayed_items[vidx] {
            ui.menu_button("Name", |ui| {
                let name_types = vec![
                    SignalNameType::Local,
                    SignalNameType::Global,
                    SignalNameType::Unique,
                ];
                let signal_name_type = signal.display_name_type;
                for name_type in name_types {
                    ui.radio(signal_name_type == name_type, name_type.to_string())
                        .clicked()
                        .then(|| {
                            ui.close_menu();
                            msgs.push(Message::ChangeSignalNameType(Some(vidx), name_type));
                        });
                }
            });
        }

        if ui.button("Remove").clicked() {
            msgs.push(Message::RemoveItem(vidx, 1));
            msgs.push(Message::InvalidateCount);
            ui.close_menu();
        }

        if path.is_none() {
            if ui.button("Rename").clicked() {
                ui.close_menu();
                msgs.push(Message::RenameItem(vidx));
            }
        }
    }

    fn add_format_menu(&self, path: &FieldRef, msgs: &mut Vec<Message>, ui: &mut egui::Ui) {
        // Should not call this unless a signal is selected, and, hence, a VCD is loaded
        let Some(waves) = self.waves.as_ref() else {
            return;
        };

        let mut available_translators = if path.field.is_empty() {
            self.translators
                .all_translator_names()
                .into_iter()
                .filter(|translator_name| {
                    let t = self.translators.get_translator(translator_name);

                    if self
                        .blacklisted_translators
                        .contains(&(path.root.clone(), (*translator_name).clone()))
                    {
                        false
                    } else {
                        match waves
                            .inner
                            .signal_meta(&path.root)
                            .and_then(|meta| t.translates(&meta))
                            .context(format!(
                                "Failed to check if {translator_name} translates {:?}",
                                path.root.full_path(),
                            )) {
                            Ok(TranslationPreference::Yes) => true,
                            Ok(TranslationPreference::Prefer) => true,
                            Ok(TranslationPreference::No) => false,
                            Err(e) => {
                                msgs.push(Message::BlacklistTranslator(
                                    path.root.clone(),
                                    (*translator_name).clone(),
                                ));
                                msgs.push(Message::Error(e));
                                false
                            }
                        }
                    }
                })
                .collect()
        } else {
            self.translators.basic_translator_names()
        };

        available_translators.sort_by(|a, b| human_sort::compare(a, b));
        let format_menu = available_translators
            .iter()
            .map(|t| (*t, Message::SignalFormatChange(path.clone(), t.to_string())))
            .collect::<Vec<_>>();

        ui.menu_button("Format", |ui| {
            for (name, msg) in format_menu {
                ui.button(name).clicked().then(|| {
                    ui.close_menu();
                    msgs.push(msg);
                });
            }
        });
    }

    fn draw_var_values(
        &self,
        item_offsets: &[ItemDrawingInfo],
        waves: &WaveData,
        ui: &mut egui::Ui,
        msgs: &mut Vec<Message>,
    ) {
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        let container_rect = Rect::from_min_size(Pos2::ZERO, response.rect.size());
        let to_screen = RectTransform::from_to(container_rect, response.rect);
        let cfg = DrawConfig {
            canvas_height: response.rect.size().y,
            line_height: 16.,
            max_transition_width: 6,
        };
        let frame_width = response.rect.width();

        let ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            // This 0.5 is very odd, but it fixes the lines we draw being smushed out across two
            // pixels, resulting in dimmer colors https://github.com/emilk/egui/issues/1322
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y) + Vec2::new(0.5, 0.5)),
            theme: &self.config.theme,
        };

        let gap = self.get_item_gap(item_offsets, &ctx);
        if let Some(cursor) = &waves.cursor {
            ui.allocate_ui_at_rect(response.rect, |ui| {
                let text_style = TextStyle::Monospace;
                ui.style_mut().override_text_style = Some(text_style);
                for (vidx, drawing_info) in item_offsets
                    .iter()
                    .sorted_by_key(|o| o.offset() as i32)
                    .enumerate()
                {
                    let next_y = ui.cursor().top();
                    // In order to align the text in this view with the variable tree,
                    // we need to keep track of how far away from the expected offset we are,
                    // and compensate for it
                    if next_y < drawing_info.offset() {
                        ui.add_space(drawing_info.offset() - next_y);
                    }

                    let y_offset = drawing_info.offset() - to_screen.transform_pos(Pos2::ZERO).y;

                    self.draw_background(vidx, waves, drawing_info, y_offset, &ctx, gap, frame_width);
                    match drawing_info {
                        ItemDrawingInfo::Signal(drawing_info) => {
                            if cursor < &0.to_bigint() {
                                break;
                            }

                            let translator =
                                waves.signal_translator(&drawing_info.field_ref, &self.translators);

                            let signal = &drawing_info.field_ref.root;
                            let meta = waves.inner.signal_meta(&signal);
                            let translation_result = waves
                                .inner
                                .query_signal(&signal, &num::BigInt::to_biguint(&cursor).unwrap())
                                .ok()
                                .flatten()
                                .map(|(_time, value)| {
                                    meta.and_then(|meta| translator.translate(&meta, &value))
                                });

                            if let Some(Ok(s)) = translation_result {
                                let subfields = s
                                    .flatten(
                                        FieldRef::without_fields(
                                            drawing_info.field_ref.root.clone(),
                                        ),
                                        &waves.signal_format,
                                        &self.translators,
                                    )
                                    .as_fields();

                                let subfield = subfields
                                    .iter()
                                    .find(|(k, _)| k == &drawing_info.field_ref.field);

                                if let Some((_, Some((v, _)))) = subfield {
                                    ui.label(v).context_menu(|ui| {
                                        self.item_context_menu(
                                            // TODO: I'm pretty sure this is wrong, we shouldn't
                                            // create a root field here
                                            Some(&FieldRef::without_fields(signal.clone())),
                                            msgs,
                                            ui,
                                            vidx,
                                        );
                                    });
                                } else {
                                    ui.label("-");
                                }
                            }
                        }
                        ItemDrawingInfo::Divider(_) => {}
                        ItemDrawingInfo::Cursor(extra_cursor) => {
                            let delta = time_string(
                                &(cursor
                                    - waves
                                        .cursors
                                        .get(&extra_cursor.idx)
                                        .unwrap_or(&BigInt::from(0))),
                                &waves.inner.metadata(),
                                &self.wanted_timescale,
                            );

                            ui.label(format!("Δ: {delta}",)).context_menu(|ui| {
                                self.item_context_menu(None, msgs, ui, vidx);
                            });
                        }
                    }
                }
            });
        } else {
            for (vidx, drawing_info) in item_offsets.iter().enumerate() {
                let y_offset = drawing_info.offset() - to_screen.transform_pos(Pos2::ZERO).y;
                self.draw_background(vidx, waves, drawing_info, y_offset, &ctx, gap, frame_width);
            }
        }
    }

    pub fn get_item_gap(&self, item_offsets: &[ItemDrawingInfo], ctx: &DrawingContext<'_>) -> f32 {
        if item_offsets.len() >= 2.max(self.config.theme.alt_frequency) {
            // Assume that first signal has standard height (for now)
            (item_offsets.get(1).unwrap().offset()
                - item_offsets.get(0).unwrap().offset()
                - ctx.cfg.line_height)
                / 2.0
        } else {
            0.0
        }
    }

    fn draw_background(
        &self,
        vidx: usize,
        vcd: &WaveData,
        drawing_info: &ItemDrawingInfo,
        y_offset: f32,
        ctx: &DrawingContext<'_>,
        gap: f32,
        frame_width: f32,
    ) {
        let default_background_color = self.get_default_alternating_background_color(vidx);
        let background_color = *vcd
            .displayed_items
            .get(drawing_info.signal_list_idx())
            .and_then(|signal| signal.background_color())
            .and_then(|color| self.config.theme.colors.get(&color))
            .unwrap_or(&default_background_color);
        // Draw background
        let min = (ctx.to_screen)(0.0, y_offset - gap);
        let max = (ctx.to_screen)(frame_width, y_offset + ctx.cfg.line_height + gap);
        ctx.painter
            .rect_filled(Rect { min, max }, Rounding::ZERO, background_color);
    }

    pub fn get_default_alternating_background_color(&self, vidx: usize) -> Color32 {
        // Set background color
        if self.config.theme.alt_frequency != 0 && (vidx / self.config.theme.alt_frequency) % 2 == 1
        {
            self.config.theme.canvas_colors.alt_background
        } else {
            Color32::TRANSPARENT
        }
    }

    fn controls_listing(&self, ui: &mut egui::Ui) {
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

        ui.add_space(20.);
        ui.label(RichText::new("Hint: You can repeat keybinds by typing Alt+0-9 before them. For example, Alt+1 Alt+0 k scrolls 10 steps up."));
    }

    fn key_listing(&self, ui: &mut egui::Ui) {
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

        ui.add_space(20.);
        ui.label(RichText::new("Hint: You can repeat keybinds by typing Alt+0-9 before them. For example, Alt+1 Alt+0 k scrolls 10 steps up."));
    }

    fn help_message(&self, ui: &mut egui::Ui) {
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

        self.controls_listing(ui);

        ui.add_space(20.0);
        ui.separator();
        ui.add_space(20.0);
        if let Some(vcd) = &self.waves {
            ui.label(RichText::new(format!("Filename: {}", vcd.source)).monospace());
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

    fn draw_menu(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
        menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("Open file...").clicked() {
                    msgs.push(Message::OpenFileDialog);
                    ui.close_menu();
                }
                if ui.button("Open URL...").clicked() {
                    msgs.push(Message::SetUrlEntryVisible(true));
                    ui.close_menu();
                }
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("Exit").clicked() {
                    msgs.push(Message::Exit);
                    ui.close_menu();
                }
            });
            ui.menu_button("View", |ui| {
                if ui
                    .add(egui::Button::new("Zoom in").shortcut_text("+"))
                    .clicked()
                {
                    msgs.push(Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 0.5,
                    });
                }
                if ui
                    .add(egui::Button::new("Zoom out").shortcut_text("-"))
                    .clicked()
                {
                    msgs.push(Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 2.0,
                    });
                }
                if ui.button("Zoom to fit").clicked() {
                    ui.close_menu();
                    msgs.push(Message::ZoomToFit);
                }
                ui.separator();
                if ui
                    .add(egui::Button::new("Scroll to start").shortcut_text("s"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::GoToStart);
                }
                if ui
                    .add(egui::Button::new("Scroll to end").shortcut_text("e"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::GoToEnd);
                }
                ui.separator();
                if ui
                    .add(egui::Button::new("Toggle side panel").shortcut_text("b"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::ToggleSidePanel);
                }
                if ui
                    .add(egui::Button::new("Toggle menu").shortcut_text("m"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::ToggleMenu);
                }
                #[cfg(not(target_arch = "wasm32"))]
                if ui
                    .add(egui::Button::new("Toggle full screen").shortcut_text("F11"))
                    .clicked()
                {
                    ui.close_menu();
                    msgs.push(Message::ToggleFullscreen);
                }
            });
            ui.menu_button("Settings", |ui| {
                ui.menu_button("Clock highlighting", |ui| {
                    let highlight_types = vec![
                        ClockHighlightType::Line,
                        ClockHighlightType::Cycle,
                        ClockHighlightType::None,
                    ];
                    for highlight_type in highlight_types {
                        ui.radio(
                            highlight_type == self.config.default_clock_highlight_type,
                            highlight_type.to_string(),
                        )
                        .clicked()
                        .then(|| {
                            ui.close_menu();
                            msgs.push(Message::SetClockHighlightType(highlight_type));
                        });
                    }
                });
                ui.menu_button("Time scale", |ui| {
                    timescale_menu(ui, msgs, &self.wanted_timescale);
                });
                if let Some(waves) = &self.waves {
                    let signal_name_type = waves.default_signal_name_type;
                    ui.menu_button("Signal names", |ui| {
                        let name_types = vec![
                            SignalNameType::Local,
                            SignalNameType::Global,
                            SignalNameType::Unique,
                        ];
                        for name_type in name_types {
                            ui.radio(signal_name_type == name_type, name_type.to_string())
                                .clicked()
                                .then(|| {
                                    ui.close_menu();
                                    msgs.push(Message::ForceSignalNameTypes(name_type));
                                });
                        }
                    });
                }
                ui.menu_button("Signal filter type", |ui| {
                    signal_filter_type_menu(ui, msgs, &self.signal_filter_type);
                });
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Control keys").clicked() {
                    ui.close_menu();
                    msgs.push(Message::SetKeyHelpVisible(true));
                }
                if ui.button("Mouse gestures").clicked() {
                    ui.close_menu();
                    msgs.push(Message::SetGestureHelpVisible(true));
                }
                ui.separator();
                if ui.button("About").clicked() {
                    ui.close_menu();
                    msgs.push(Message::SetAboutVisible(true));
                }
            });
        });
    }
}

fn signal_filter_type_menu(
    ui: &mut Ui,
    msgs: &mut Vec<Message>,
    signal_filter_type: &SignalFilterType,
) {
    let filter_types = vec![
        SignalFilterType::Fuzzy,
        SignalFilterType::Regex,
        SignalFilterType::Start,
        SignalFilterType::Contain,
    ];
    for filter_type in filter_types {
        ui.radio(*signal_filter_type == filter_type, filter_type.to_string())
            .clicked()
            .then(|| {
                ui.close_menu();
                msgs.push(Message::SetSignalFilterType(filter_type));
            });
    }
}

fn timescale_menu(ui: &mut egui::Ui, msgs: &mut Vec<Message>, wanted_timescale: &Timescale) {
    let timescales = vec![
        Timescale::Fs,
        Timescale::Ps,
        Timescale::Ns,
        Timescale::Us,
        Timescale::Ms,
        Timescale::S,
    ];
    for timescale in timescales {
        ui.radio(*wanted_timescale == timescale, timescale.to_string())
            .clicked()
            .then(|| {
                ui.close_menu();
                msgs.push(Message::SetTimeScale(timescale));
            });
    }
}

pub fn time_string(time: &BigInt, metadata: &Metadata, wanted_timescale: &Timescale) -> String {
    let wanted_exponent = timescale_to_exponent(wanted_timescale);
    let data_exponent = timescale_to_exponent(&metadata.timescale.1);
    let exponent_diff = wanted_exponent - data_exponent;
    if exponent_diff >= 0 {
        let precision = exponent_diff as usize;
        format!(
            "{scaledtime:.precision$} {wanted_timescale}",
            scaledtime = BigRational::new(
                time * metadata.timescale.0.unwrap_or(1),
                (BigInt::from(10)).pow(exponent_diff as u32)
            )
            .to_f64()
            .unwrap_or(f64::NAN)
        )
    } else {
        format!(
            "{scaledtime} {wanted_timescale}",
            scaledtime = time
                * metadata.timescale.0.unwrap_or(1)
                * (BigInt::from(10)).pow(-exponent_diff as u32)
        )
    }
}

fn timescale_to_exponent(timescale: &Timescale) -> i8 {
    match timescale {
        Timescale::Fs => -15,
        Timescale::Ps => -12,
        Timescale::Ns => -9,
        Timescale::Us => -6,
        Timescale::Ms => -3,
        Timescale::S => 0,
        _ => 0,
    }
}
