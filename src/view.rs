use color_eyre::eyre::Context;
use eframe::egui::{self, style::Margin, Align, Color32, Layout, Painter, RichText};
use eframe::egui::{Frame, Sense, TextStyle};
use eframe::emath::RectTransform;
use eframe::epaint::{Pos2, Rect, Rounding, Vec2};
use itertools::Itertools;
use log::{info, warn};
use num::BigInt;
use spade_common::num_ext::InfallibleToBigInt;

use crate::config::SurferTheme;
use crate::displayed_item::{draw_rename_window, DisplayedItem};
use crate::help::{draw_about_window, draw_control_help_window};
use crate::signal_filter::filtered_signals;
use crate::time::{time_string, timescale_menu};
use crate::util::uint_idx_to_alpha_idx;
use crate::wave_container::{FieldRef, ModuleRef};
use crate::wave_source::draw_progress_panel;
use crate::{
    command_prompt::show_command_prompt, translation::SignalInfo, wave_data::WaveData, Message,
    MoveDir, State,
};

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
        if let Some(waves) = &self.waves {
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
                            ui.label(&waves.source.to_string());
                            if let Some(datetime) = waves.inner.metadata().date {
                                ui.add_space(10.0);
                                ui.label(format!("Generated: {datetime}"));
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::RIGHT), |ui| {
                            if let Some(time) = &waves.cursor {
                                ui.label(time_string(
                                    time,
                                    &waves.inner.metadata(),
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
                                            if let Some(waves) = &self.waves {
                                                self.draw_all_scopes(&mut msgs, waves, ui);
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
                                        self.draw_signal_filter_edit(ui, filter, &mut msgs);
                                    });
                                    ui.add_space(3.0);

                                    egui::ScrollArea::both()
                                        .id_source("signals")
                                        .show(ui, |ui| {
                                            if let Some(waves) = &self.waves {
                                                self.draw_signal_list(&mut msgs, waves, ui, filter);
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
            draw_progress_panel(ctx, vcd_progress_data);
        }

        if let Some(waves) = &self.waves {
            if !waves.displayed_items.is_empty() {
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
                            |ui| self.draw_item_list(&mut msgs, &waves, ui),
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
                                    self.draw_var_values(&item_offsets, waves, ui, &mut msgs)
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
                    draw_rename_window(
                        ctx,
                        &mut msgs,
                        idx,
                        &mut *self.item_renaming_string.borrow_mut(),
                    );
                }
            }
        };

        if self.waves.is_none()
            || self
                .waves
                .as_ref()
                .map_or(false, |waves| waves.displayed_items.is_empty())
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
            draw_about_window(ctx, &mut msgs);
        }

        if self.show_keys {
            draw_control_help_window(ctx, max_width, max_height, &mut msgs);
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
            .context("Failed to get child modules")
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
            .context("Failed to get child modules")
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
        for sig in filtered_signals(wave, filter, &self.signal_filter_type) {
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
        waves: &WaveData,
        ui: &mut egui::Ui,
    ) -> Vec<ItemDrawingInfo> {
        let mut item_offsets = Vec::new();

        for (vidx, displayed_item) in waves.displayed_items.iter().enumerate().skip(waves.scroll) {
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
                        self.draw_plain_var(msgs, vidx, &displayed_item, &mut item_offsets, ui);
                    }
                    DisplayedItem::Cursor(_) => {
                        self.draw_plain_var(msgs, vidx, &displayed_item, &mut item_offsets, ui);
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
                .map_or(0, |waves| waves.displayed_items.len()),
        );
        ui.label(
            egui::RichText::new(alpha_id)
                .background_color(self.config.theme.accent_warn.background)
                .monospace()
                .color(self.config.theme.accent_warn.foreground),
        );
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

                    self.draw_background(
                        vidx,
                        waves,
                        drawing_info,
                        y_offset,
                        &ctx,
                        gap,
                        frame_width,
                    );
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
        waves: &WaveData,
        drawing_info: &ItemDrawingInfo,
        y_offset: f32,
        ctx: &DrawingContext<'_>,
        gap: f32,
        frame_width: f32,
    ) {
        let default_background_color = self.get_default_alternating_background_color(vidx);
        let background_color = *waves
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
}
