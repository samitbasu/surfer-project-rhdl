use color_eyre::eyre::Context;
use eframe::egui::{self, style::Margin, Align, Color32, Event, Key, Layout, Painter, RichText};
use eframe::egui::{menu, Frame, Grid, Sense, TextStyle};
use eframe::emath;
use eframe::epaint::{Pos2, Rect, Rounding, Vec2};
use fastwave_backend::{Metadata, SignalIdx, Timescale};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use itertools::Itertools;
use log::info;
use num::{BigInt, BigRational, ToPrimitive};
use spade_common::num_ext::InfallibleToBigInt;

use crate::config::SurferTheme;
use crate::descriptors::PathDescriptor;
use crate::util::uint_idx_to_alpha_idx;
use crate::{
    command_prompt::show_command_prompt,
    translation::{SignalInfo, TranslationPreference},
    Message, MoveDir, SignalDescriptor, State, VcdData,
};
use crate::{ClockHighlightType, DisplayedItem, LoadProgress, SignalNameType};

/// Index used to keep track of traces and their sub-traces
pub(crate) type TraceIdx = (SignalIdx, Vec<String>);

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
    pub tidx: TraceIdx,
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
        if let Some(vcd) = &self.vcd {
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
                            ui.label(&vcd.filename);
                            if let Some(datetime) = vcd.inner.metadata.date {
                                ui.add_space(10.0);
                                ui.label(format!("Generated: {datetime}"));
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::RIGHT), |ui| {
                            if let Some(time) = &vcd.cursor {
                                ui.label(time_string(
                                    time,
                                    &vcd.inner.metadata,
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
                                            if let Some(vcd) = &self.vcd {
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
                                            ui.button("❌")
                                                .on_hover_text("Clear filter")
                                                .clicked()
                                                .then(|| filter.clear());
                                            let response = ui.add(
                                                egui::TextEdit::singleline(filter)
                                                    .hint_text("Filter"),
                                            );
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
                                            if let Some(vcd) = &self.vcd {
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

        if let Some(vcd) = &self.vcd {
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
            }
        };

        if self.vcd.is_none()
            || self
                .vcd
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

        if self.show_url_entry {
            let mut open = true;
            egui::Window::new("Load URL")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        let url = &mut *self.url.borrow_mut();
                        ui.text_edit_singleline(url);
                        ui.horizontal(|ui| {
                            if ui.button("Load URL").clicked() {
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
                    self.filter_focused,
                ) {
                    (Key::Num0, true, false, false) => {
                        handle_digit(0, modifiers, &mut msgs);
                    }
                    (Key::Num1, true, false, false) => {
                        handle_digit(1, modifiers, &mut msgs);
                    }
                    (Key::Num2, true, false, false) => {
                        handle_digit(2, modifiers, &mut msgs);
                    }
                    (Key::Num3, true, false, false) => {
                        handle_digit(3, modifiers, &mut msgs);
                    }
                    (Key::Num4, true, false, false) => {
                        handle_digit(4, modifiers, &mut msgs);
                    }
                    (Key::Num5, true, false, false) => {
                        handle_digit(5, modifiers, &mut msgs);
                    }
                    (Key::Num6, true, false, false) => {
                        handle_digit(6, modifiers, &mut msgs);
                    }
                    (Key::Num7, true, false, false) => {
                        handle_digit(7, modifiers, &mut msgs);
                    }
                    (Key::Num8, true, false, false) => {
                        handle_digit(8, modifiers, &mut msgs);
                    }
                    (Key::Num9, true, false, false) => {
                        handle_digit(9, modifiers, &mut msgs);
                    }
                    (Key::Home, true, false, false) => msgs.push(Message::SetVerticalScroll(0)),
                    (Key::End, true, false, false) => {
                        if let Some(vcd) = &self.vcd {
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
                        if let Some(vcd) = &self.vcd {
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

        msgs
    }
}

fn handle_digit(digit: u8, modifiers: &egui::Modifiers, msgs: &mut Vec<Message>) {
    if modifiers.alt {
        msgs.push(Message::SetCursorPosition(digit))
    } else if modifiers.ctrl {
        msgs.push(Message::GoToCursorPosition(digit))
    } else {
        msgs.push(Message::AddCount((digit + 48) as char))
    }
}

impl State {
    pub fn draw_all_scopes(&self, msgs: &mut Vec<Message>, vcd: &VcdData, ui: &mut egui::Ui) {
        for idx in vcd.inner.root_scopes_by_idx() {
            self.draw_selectable_child_or_orphan_scope(msgs, vcd, idx, ui);
        }
    }

    fn draw_selectable_child_or_orphan_scope(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VcdData,
        scope_idx: fastwave_backend::ScopeIdx,
        ui: &mut egui::Ui,
    ) {
        let name = vcd.inner.scope_name_by_idx(scope_idx);
        let fastwave_backend::ScopeIdx(idx) = scope_idx;
        if vcd.inner.child_scopes_by_idx(scope_idx).is_empty() {
            ui.add(egui::SelectableLabel::new(
                vcd.active_scope == Some(scope_idx),
                name,
            ))
            .clicked()
            .then(|| msgs.push(Message::SetActiveScope(scope_idx.into())));
        } else {
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                egui::Id::new(idx),
                false,
            )
            .show_header(ui, |ui| {
                ui.with_layout(
                    Layout::top_down(Align::LEFT).with_cross_justify(true),
                    |ui| {
                        ui.add(egui::SelectableLabel::new(
                            vcd.active_scope == Some(scope_idx),
                            name,
                        ))
                        .clicked()
                        .then(|| msgs.push(Message::SetActiveScope(scope_idx.into())))
                    },
                );
            })
            .body(|ui| self.draw_root_scope_view(msgs, vcd, scope_idx, ui));
        }
    }

    fn draw_root_scope_view(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VcdData,
        root_idx: fastwave_backend::ScopeIdx,
        ui: &mut egui::Ui,
    ) {
        for child_scope_idx in vcd.inner.child_scopes_by_idx(root_idx) {
            self.draw_selectable_child_or_orphan_scope(msgs, vcd, child_scope_idx, ui);
        }
    }

    fn draw_signal_list(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VcdData,
        ui: &mut egui::Ui,
        filter: &str,
    ) {
        if let Some(idx) = vcd.active_scope {
            let matcher = SkimMatcherV2::default();
            let signals = vcd.inner.get_children_signal_idxs(idx);
            let listed = signals
                .iter()
                .filter_map(|sig| {
                    let name = vcd.inner.signal_from_signal_idx(*sig).name();
                    if (!name.starts_with("_e_"))
                        && matcher.fuzzy_match(name.as_str(), filter).is_some()
                    {
                        Some((sig, name.clone()))
                    } else {
                        None
                    }
                })
                .sorted_by(|a, b| human_sort::compare(&a.1, &b.1));

            for (sig, name) in listed {
                ui.with_layout(
                    Layout::top_down(Align::LEFT).with_cross_justify(true),
                    |ui| {
                        ui.add(egui::SelectableLabel::new(false, name))
                            .clicked()
                            .then(|| msgs.push(Message::AddSignal(SignalDescriptor::Id(*sig))));
                    },
                );
            }
        }
    }

    fn draw_item_list(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VcdData,
        ui: &mut egui::Ui,
    ) -> Vec<ItemDrawingInfo> {
        let mut item_offsets = Vec::new();

        for (vidx, displayed_item) in vcd.displayed_items.iter().enumerate().skip(vcd.scroll) {
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| match displayed_item {
                    DisplayedItem::Signal(displayed_signal) => {
                        let sig = displayed_signal.idx;
                        let info = &displayed_signal.info;
                        let signal = vcd.inner.signal_from_signal_idx(sig);

                        self.draw_signal_var(
                            msgs,
                            vidx,
                            &displayed_signal.display_name,
                            &(signal.real_idx(), vec![]),
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
        path: &(SignalIdx, Vec<String>),
        item_offsets: &mut Vec<ItemDrawingInfo>,
        info: &SignalInfo,
        ui: &mut egui::Ui,
    ) {
        let mut draw_label = |ui: &mut egui::Ui| {
            let tooltip = if let Some(vcd) = &self.vcd {
                if path.1.len() == 0 {
                    format!(
                        "{}\nNum bits: {}",
                        vcd.ids_to_fullnames.get(&path.0).unwrap_or(&"".to_string()),
                        vcd.inner
                            .signal_from_signal_idx(path.0)
                            .num_bits()
                            .map(|v| format!("{v}"))
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
                        self.item_context_menu(Some(path), msgs, ui, vidx);
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
                    egui::Id::new(&path),
                    false,
                )
                .show_header(ui, draw_label)
                .body(|ui| {
                    for (name, info) in subfields {
                        let mut new_path = path.clone();
                        new_path.1.push(name.clone());
                        self.draw_signal_var(msgs, vidx, name, &new_path, item_offsets, info, ui);
                    }
                });

                let offset = response.0.rect.top();
                item_offsets.push(ItemDrawingInfo::Signal(SignalDrawingInfo {
                    tidx: path.clone(),
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
                    tidx: path.clone(),
                    signal_list_idx: vidx,
                    offset: label.inner.rect.top(),
                }));
            }
        }
    }

    fn add_focus_marker(&self, vidx: usize, ui: &mut egui::Ui) {
        let focus_marker_color = if self
            .vcd
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
                        egui::RichText::new(displayed_item.name().clone()).color(*text_color),
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
            self.vcd.as_ref().map_or(0, |vcd| vcd.displayed_items.len()),
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
        path: Option<&(SignalIdx, Vec<String>)>,
        msgs: &mut Vec<Message>,
        ui: &mut egui::Ui,
        vidx: usize,
    ) {
        if let Some(path) = path {
            self.add_format_menu(path, msgs, ui);
        }

        let displayed_item = &self.vcd.as_ref().unwrap().displayed_items[vidx];
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

        if let DisplayedItem::Signal(signal) = &self.vcd.as_ref().unwrap().displayed_items[vidx] {
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
    }

    fn add_format_menu(
        &self,
        path: &(SignalIdx, Vec<String>),
        msgs: &mut Vec<Message>,
        ui: &mut egui::Ui,
    ) {
        // Should not call this unless a signal is selected, and, hence, a VCD is loaded
        let Some(vcd) = self.vcd.as_ref() else {
            return;
        };

        let mut available_translators = if path.1.is_empty() {
            self.translators
                .all_translator_names()
                .into_iter()
                .filter(|translator_name| {
                    let t = self.translators.get_translator(translator_name);

                    if self
                        .blacklisted_translators
                        .contains(&(path.0, (*translator_name).clone()))
                    {
                        false
                    } else {
                        let sig = vcd.inner.signal_from_signal_idx(path.0);

                        match t.translates(&sig).context(format!(
                            "Failed to check if {translator_name} translates {:?}",
                            sig.path(),
                        )) {
                            Ok(TranslationPreference::Yes) => true,
                            Ok(TranslationPreference::Prefer) => true,
                            Ok(TranslationPreference::No) => false,
                            Err(e) => {
                                msgs.push(Message::BlacklistTranslator(
                                    path.0,
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
            .map(|t| {
                (
                    *t,
                    Message::SignalFormatChange(PathDescriptor::from_traceidx(path), t.to_string()),
                )
            })
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
        vcd: &VcdData,
        ui: &mut egui::Ui,
        msgs: &mut Vec<Message>,
    ) {
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        let container_rect = Rect::from_min_size(Pos2::ZERO, response.rect.size());
        let to_screen = emath::RectTransform::from_to(container_rect, response.rect);
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
        if let Some(cursor) = &vcd.cursor {
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

                    self.draw_background(
                        vidx,
                        vcd,
                        drawing_info,
                        to_screen,
                        &ctx,
                        gap,
                        frame_width,
                    );
                    match drawing_info {
                        ItemDrawingInfo::Signal(drawing_info) => {
                            if cursor < &0.to_bigint() {
                                break;
                            }

                            let translator = vcd.signal_translator(
                                (drawing_info.tidx.0, vec![]),
                                &self.translators,
                            );

                            let signal = vcd.inner.signal_from_signal_idx(drawing_info.tidx.0);

                            let translation_result = signal
                                .query_val_on_tmln(
                                    &num::BigInt::to_biguint(&cursor).unwrap(),
                                    &vcd.inner,
                                )
                                .map(|(_time, value)| translator.translate(&signal, &value));

                            if let Ok(Ok(s)) = translation_result {
                                let subfields = s
                                    .flatten(
                                        (drawing_info.tidx.0, vec![]),
                                        &vcd.signal_format,
                                        &self.translators,
                                    )
                                    .as_fields();

                                let subfield =
                                    subfields.iter().find(|(k, _)| k == &drawing_info.tidx.1);

                                if let Some((_, Some((v, _)))) = subfield {
                                    ui.label(v).context_menu(|ui| {
                                        self.item_context_menu(
                                            Some(&(signal.real_idx(), vec![])),
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
                                    - vcd
                                        .cursors
                                        .get(&extra_cursor.idx)
                                        .unwrap_or(&BigInt::from(0))),
                                &vcd.inner.metadata,
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
                self.draw_background(vidx, vcd, drawing_info, to_screen, &ctx, gap, frame_width);
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
        vcd: &VcdData,
        drawing_info: &ItemDrawingInfo,
        to_screen: emath::RectTransform,
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
        // We draw in absolute coords, but the signal offset in the y
        // direction is also in absolute coordinates, so we need to
        // compensate for that
        let y_offset = drawing_info.offset() - to_screen.transform_pos(Pos2::ZERO).y;
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
        ui.label(RichText::new("Hint: You can repeat keybinds by typing a number before them. For example, 10k scrolls 10 steps up."));
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
            ("", "Alt+0-9", "Add numbered cursor"),
            ("", "Ctrl+0-9", "Center view at numbered cursor"),
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
        ui.label(RichText::new("Hint: You can repeat keybinds by typing a number before them. For example, 10k scrolls 10 steps up."));
    }

    fn help_message(&self, ui: &mut egui::Ui) {
        if self.vcd.is_none() {
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
        if let Some(vcd) = &self.vcd {
            ui.label(RichText::new(format!("Filename: {}", vcd.filename)).monospace());
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
                if let Some(vcd) = &self.vcd {
                    let signal_name_type = vcd.default_signal_name_type;
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
                ui.separator();
                ui.menu_button("Time scale", |ui| {
                    timescale_menu(ui, msgs, &self.wanted_timescale);
                });
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Control keys").clicked() {
                    ui.close_menu();
                    msgs.push(Message::SetKeyHelpVisible(true));
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
