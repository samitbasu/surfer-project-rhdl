use color_eyre::eyre::Context;
#[cfg(not(target_arch = "wasm32"))]
use eframe::egui::ViewportCommand;
use eframe::egui::{self, style::Margin, Align, Layout, Painter, RichText};
use eframe::egui::{FontSelection, ScrollArea, Sense, Style, TextStyle, WidgetText};
use eframe::emath::RectTransform;
use eframe::epaint::text::LayoutJob;
use eframe::epaint::{Color32, Pos2, Rect, Rounding, Vec2};
use egui_extras::{Column, TableBuilder, TableRow};
use fzcmd::expand_command;
use itertools::Itertools;
use log::{info, warn};

use crate::benchmark::NUM_PERF_SAMPLES;
use crate::command_prompt::get_parser;
use crate::config::SurferTheme;
use crate::displayed_item::{draw_rename_window, DisplayedItem};
use crate::help::{draw_about_window, draw_control_help_window, draw_quickstart_help_window};
use crate::logs::EGUI_LOGGER;
use crate::time::time_string;
use crate::translation::{SubFieldFlatTranslationResult, TranslatedValue};
use crate::util::uint_idx_to_alpha_idx;
use crate::variable_name_filter::filtered_variables;
use crate::wave_container::{FieldRef, ScopeRef, VariableRef};
use crate::wave_source::{draw_progress_panel, LoadOptions};
use crate::{
    command_prompt::show_command_prompt, translation::VariableInfo, wave_data::WaveData, Message,
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
    pub text_size: f32,
    pub max_transition_width: i32,
}

impl DrawConfig {
    pub fn new(canvas_height: f32) -> Self {
        let line_height = 16.;
        Self {
            canvas_height,
            line_height,
            text_size: line_height - 5.,
            max_transition_width: 6,
        }
    }
}

#[derive(Debug)]
pub struct VariableDrawingInfo {
    pub field_ref: FieldRef,
    pub item_list_idx: usize,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct DividerDrawingInfo {
    pub item_list_idx: usize,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct CursorDrawingInfo {
    pub item_list_idx: usize,
    pub top: f32,
    pub bottom: f32,
    pub idx: u8,
}

#[derive(Debug)]
pub struct TimeLineDrawingInfo {
    pub item_list_idx: usize,
    pub top: f32,
    pub bottom: f32,
}

pub enum ItemDrawingInfo {
    Variable(VariableDrawingInfo),
    Divider(DividerDrawingInfo),
    Cursor(CursorDrawingInfo),
    TimeLine(TimeLineDrawingInfo),
}

impl ItemDrawingInfo {
    pub fn top(&self) -> f32 {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Cursor(drawing_info) => drawing_info.top,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.top,
        }
    }
    pub fn bottom(&self) -> f32 {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Cursor(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.bottom,
        }
    }
    pub fn item_list_idx(&self) -> usize {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.item_list_idx,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.item_list_idx,
            ItemDrawingInfo::Cursor(drawing_info) => drawing_info.item_list_idx,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.item_list_idx,
        }
    }
}

impl eframe::App for State {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.sys.timing.borrow_mut().start_frame();

        if self.sys.continuous_redraw {
            self.invalidate_draw_commands();
        }

        let (fullscreen, window_size) = ctx.input(|i| {
            (
                i.viewport().fullscreen.unwrap_or_default(),
                Some(i.screen_rect.size()),
            )
        });

        self.sys.timing.borrow_mut().start("draw");
        let mut msgs = self.draw(ctx, window_size);
        self.sys.timing.borrow_mut().end("draw");

        self.sys.timing.borrow_mut().start("update");
        if let Some(scale) = self.ui_scale {
            if ctx.pixels_per_point() != scale {
                ctx.set_pixels_per_point(scale)
            }
        }

        while let Some(msg) = msgs.pop() {
            #[cfg(not(target_arch = "wasm32"))]
            if let Message::Exit = msg {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Message::ToggleFullscreen = msg {
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!fullscreen));
            }
            self.update(msg);
        }
        self.sys.timing.borrow_mut().end("update");

        self.sys.timing.borrow_mut().start("handle_async_messages");
        self.handle_async_messages();
        self.sys.timing.borrow_mut().end("handle_async_messages");

        // We can save some user battery life by not redrawing unless needed. At the moment,
        // we only need to continuously redraw to make surfer interactive during loading, otherwise
        // we'll back off a bit
        if self.sys.continuous_redraw || self.sys.vcd_progress.is_some() || self.show_performance {
            ctx.request_repaint();
        }

        if let Some(prev_cpu) = frame.info().cpu_usage {
            self.sys.rendering_cpu_times.push_back(prev_cpu);
            if self.sys.rendering_cpu_times.len() > NUM_PERF_SAMPLES {
                self.sys.rendering_cpu_times.pop_front();
            }
        }

        self.sys.timing.borrow_mut().end_frame();
    }
}

impl State {
    pub(crate) fn draw(&mut self, ctx: &egui::Context, window_size: Option<Vec2>) -> Vec<Message> {
        let max_width = ctx.available_rect().width();
        let max_height = ctx.available_rect().height();

        let mut msgs = vec![];

        if self.show_about {
            draw_about_window(ctx, &mut msgs);
        }

        if self.show_keys {
            draw_control_help_window(ctx, &mut msgs);
        }

        if self.show_quick_start {
            draw_quickstart_help_window(ctx, &mut msgs);
        }

        if self.show_gestures {
            self.mouse_gesture_help(ctx, &mut msgs);
        }

        if self.show_logs {
            self.draw_log_window(ctx, &mut msgs);
        }

        if self.show_performance {
            self.draw_performance_graph(ctx, &mut msgs);
        }

        if self.show_cursor_window {
            if let Some(waves) = &self.waves {
                self.draw_cursor_window(waves, ctx, &mut msgs);
            }
        }

        if let Some(idx) = self.rename_target {
            draw_rename_window(
                ctx,
                &mut msgs,
                idx,
                &mut self.sys.item_renaming_string.borrow_mut(),
            );
        }

        if self
            .show_menu
            .unwrap_or_else(|| self.config.layout.show_menu())
        {
            self.add_menu_panel(ctx, &mut msgs);
        }

        if self
            .show_toolbar
            .unwrap_or_else(|| self.config.layout.show_toolbar())
        {
            self.add_toolbar_panel(ctx, &mut msgs);
        }

        if self.show_url_entry {
            self.draw_load_url(ctx, &mut msgs);
        }

        if let Some(waves) = &self.waves {
            if self
                .show_statusbar
                .unwrap_or(self.config.layout.show_statusbar())
            {
                self.add_statusbar_panel(ctx, waves, &mut msgs);
            }
            if self
                .show_overview
                .unwrap_or(self.config.layout.show_overview())
            {
                self.add_overview_panel(ctx, waves)
            }
        }

        if self
            .show_hierarchy
            .unwrap_or(self.config.layout.show_hierarchy())
        {
            egui::SidePanel::left("variable select left panel")
                .default_width(300.)
                .width_range(100.0..=max_width)
                .frame(egui::Frame {
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

                                    ui.heading("Scopes");
                                    ui.add_space(3.0);

                                    ScrollArea::both().id_source("scopes").show(ui, |ui| {
                                        ui.style_mut().wrap = Some(false);
                                        if let Some(waves) = &self.waves {
                                            self.draw_all_scopes(&mut msgs, waves, ui);
                                        }
                                    });
                                });

                            egui::Frame::none()
                                .inner_margin(Margin::same(5.0))
                                .show(ui, |ui| {
                                    let filter = &mut *self.sys.variable_name_filter.borrow_mut();
                                    ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
                                        ui.heading("Variables");
                                        ui.add_space(3.0);
                                        self.draw_variable_name_filter_edit(ui, filter, &mut msgs);
                                    });
                                    ui.add_space(3.0);

                                    ScrollArea::both()
                                        .max_height(f32::INFINITY)
                                        .id_source("variables")
                                        .show(ui, |ui| {
                                            if let Some(waves) = &self.waves {
                                                self.draw_variable_list(
                                                    &mut msgs, waves, ui, filter,
                                                );
                                            }
                                        });
                                });

                            if self.waves.is_some() {
                                egui::TopBottomPanel::bottom("add_extra_buttons")
                                    .frame(egui::Frame {
                                        fill: self.config.theme.primary_ui_color.background,
                                        inner_margin: Margin::same(5.0),
                                        ..Default::default()
                                    })
                                    .show_inside(ui, |ui| {
                                        ui.with_layout(Layout::left_to_right(Align::LEFT), |ui| {
                                            ui.button("Add divider").clicked().then(|| {
                                                msgs.push(Message::AddDivider(None, None));
                                            });
                                            ui.button("Add timeline").clicked().then(|| {
                                                msgs.push(Message::AddTimeLine(None));
                                            });
                                        })
                                    });
                            }
                        },
                    );
                });
        }

        if self.sys.command_prompt.visible {
            show_command_prompt(self, ctx, window_size, &mut msgs);
        }

        if let Some(vcd_progress_data) = &self.sys.vcd_progress {
            draw_progress_panel(ctx, vcd_progress_data);
        }

        if self.waves.is_some() {
            let scroll_offset = self.waves.as_ref().unwrap().scroll_offset;
            if self.waves.as_ref().unwrap().any_displayed() {
                egui::SidePanel::left("variable list")
                    .default_width(200.)
                    .width_range(20.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap = Some(false);
                        self.handle_pointer_in_ui(ui, &mut msgs);
                        let response = ScrollArea::both()
                            .vertical_scroll_offset(scroll_offset)
                            .show(ui, |ui| {
                                self.draw_item_list(&mut msgs, ui);
                            });
                        self.waves.as_mut().unwrap().top_item_draw_offset =
                            response.inner_rect.min.y;
                        self.waves.as_mut().unwrap().total_height = response.inner_rect.height();
                        if (scroll_offset - response.state.offset.y).abs() > 5. {
                            msgs.push(Message::SetScrollOffset(response.state.offset.y));
                        }
                    });

                egui::SidePanel::left("variable values")
                    .default_width(100.)
                    .width_range(10.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap = Some(false);
                        self.handle_pointer_in_ui(ui, &mut msgs);
                        ui.with_layout(
                            Layout::top_down(Align::LEFT).with_cross_justify(true),
                            |ui| {
                                let response = ScrollArea::both()
                                    .vertical_scroll_offset(scroll_offset)
                                    .show(ui, |ui| self.draw_var_values(ui, &mut msgs));
                                if (scroll_offset - response.state.offset.y).abs() > 5. {
                                    msgs.push(Message::SetScrollOffset(response.state.offset.y));
                                }
                            },
                        )
                    });

                egui::CentralPanel::default()
                    .frame(egui::Frame {
                        inner_margin: Margin::same(0.0),
                        outer_margin: Margin::same(0.0),
                        ..Default::default()
                    })
                    .show(ctx, |ui| {
                        self.draw_items(&mut msgs, ui);
                    });
            }
        };

        if self.waves.is_none()
            || self
                .waves
                .as_ref()
                .map_or(false, |waves| !waves.any_displayed())
        {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(self.config.theme.canvas_colors.background))
                .show(ctx, |ui| {
                    ui.add_space(max_height * 0.1);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🏄 Surfer").monospace().size(24.));
                        ui.add_space(20.);
                        let layout = Layout::top_down(egui::Align::LEFT);
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

    fn draw_load_url(&self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        egui::Window::new("Load URL")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let url = &mut *self.sys.url.borrow_mut();
                    let response = ui.text_edit_singleline(url);
                    ui.horizontal(|ui| {
                        if ui.button("Load URL").clicked()
                            || (response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            msgs.push(Message::LoadWaveformFileFromUrl(
                                url.clone(),
                                LoadOptions::clean(),
                            ));
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

    fn handle_pointer_in_ui(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
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
    }

    pub fn draw_all_scopes(&self, msgs: &mut Vec<Message>, wave: &WaveData, ui: &mut egui::Ui) {
        for scope in wave.inner.root_scopes() {
            self.draw_selectable_child_or_orphan_scope(msgs, wave, &scope, ui);
        }
    }

    fn add_scope_selectable_label(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        scope: &ScopeRef,
        ui: &mut egui::Ui,
    ) {
        let name = scope.name();
        let mut response = ui.add(egui::SelectableLabel::new(
            wave.active_scope == Some(scope.clone()),
            name,
        ));
        if self
            .show_variable_tooltip
            .unwrap_or(self.config.layout.show_tooltip())
        {
            response = response.on_hover_text(scope_tooltip_text(wave, &scope));
        }
        response
            .clicked()
            .then(|| msgs.push(Message::SetActiveScope(scope.clone())));
    }

    fn draw_selectable_child_or_orphan_scope(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        scope: &ScopeRef,
        ui: &mut egui::Ui,
    ) {
        let Some(child_scopes) = wave
            .inner
            .child_scopes(scope)
            .context("Failed to get child scopes")
            .map_err(|e| warn!("{e:#?}"))
            .ok()
        else {
            return;
        };

        if child_scopes.is_empty() {
            self.add_scope_selectable_label(msgs, wave, scope, ui);
        } else {
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                egui::Id::new(scope),
                false,
            )
            .show_header(ui, |ui| {
                ui.with_layout(
                    Layout::top_down(Align::LEFT).with_cross_justify(true),
                    |ui| {
                        self.add_scope_selectable_label(msgs, wave, scope, ui);
                    },
                );
            })
            .body(|ui| self.draw_root_scope_view(msgs, wave, scope, ui));
        }
    }

    fn draw_root_scope_view(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        root_scope: &ScopeRef,
        ui: &mut egui::Ui,
    ) {
        let Some(child_scopes) = wave
            .inner
            .child_scopes(root_scope)
            .context("Failed to get child scopes")
            .map_err(|e| warn!("{e:#?}"))
            .ok()
        else {
            return;
        };

        for child_scope in child_scopes {
            self.draw_selectable_child_or_orphan_scope(msgs, wave, &child_scope, ui);
        }
    }

    fn draw_variable_list(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        ui: &mut egui::Ui,
        filter: &str,
    ) {
        for variable in filtered_variables(wave, filter, &self.variable_name_filter_type) {
            let index = wave
                .inner
                .variable_meta(&variable)
                .ok()
                .as_ref()
                .and_then(|meta| meta.index.clone())
                .map(|index| format!(" {index}"))
                .unwrap_or_default();

            let sig_name = format!("{}{}", variable.name.clone(), index);
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    let mut response = ui.add(egui::SelectableLabel::new(false, sig_name));
                    if self
                        .show_variable_tooltip
                        .unwrap_or(self.config.layout.show_tooltip())
                    {
                        response = response.on_hover_text(variable_tooltip_text(wave, &variable));
                    }
                    response
                        .clicked()
                        .then(|| msgs.push(Message::AddVariable(variable.clone())));
                },
            );
        }
    }

    fn draw_item_list(&mut self, msgs: &mut Vec<Message>, ui: &mut egui::Ui) {
        let mut item_offsets = Vec::new();
        let draw_alpha = self.sys.command_prompt.visible
            && expand_command(&self.sys.command_prompt_text.borrow(), get_parser(self))
                .expanded
                .starts_with("item_focus");

        let alignment = self.get_name_alignment();
        ui.with_layout(Layout::top_down(alignment).with_cross_justify(true), |ui| {
            for (vidx, displayed_item) in self
                .waves
                .as_ref()
                .unwrap()
                .displayed_items
                .iter()
                .enumerate()
            {
                match displayed_item {
                    DisplayedItem::Variable(displayed_variable) => {
                        let var = displayed_variable;
                        let info = &displayed_variable.info;
                        let index = if self
                            .show_variable_indices
                            .unwrap_or_else(|| self.config.layout.show_variable_indices())
                        {
                            self.waves
                                .as_ref()
                                .unwrap()
                                .inner
                                .variable_meta(&var.variable_ref)
                                .ok()
                                .as_ref()
                                .and_then(|meta| meta.index.clone())
                                .map(|index| format!(" {index}"))
                        } else {
                            None
                        };
                        let style = Style::default();
                        let mut layout_job = LayoutJob::default();
                        self.add_alpha_id(draw_alpha, vidx, &style, &mut layout_job, Align::LEFT);
                        displayed_item.add_to_layout_job(
                            &self.config.theme.foreground,
                            index,
                            &style,
                            &mut layout_job,
                        );

                        self.add_alpha_id(draw_alpha, vidx, &style, &mut layout_job, Align::RIGHT);

                        self.draw_variable(
                            msgs,
                            vidx,
                            WidgetText::LayoutJob(layout_job),
                            FieldRef {
                                root: var.variable_ref.clone(),
                                field: vec![],
                            },
                            &mut item_offsets,
                            info,
                            ui,
                        );
                    }
                    DisplayedItem::Divider(_) => {
                        self.draw_plain_item(
                            msgs,
                            vidx,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                            draw_alpha,
                        );
                    }
                    DisplayedItem::Cursor(_) => {
                        self.draw_plain_item(
                            msgs,
                            vidx,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                            draw_alpha,
                        );
                    }
                    DisplayedItem::Placeholder(_) => {
                        self.draw_plain_item(
                            msgs,
                            vidx,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                            draw_alpha,
                        );
                    }
                    DisplayedItem::TimeLine(_) => {
                        self.draw_plain_item(
                            msgs,
                            vidx,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                            draw_alpha,
                        );
                    }
                }
            }
        });

        self.waves.as_mut().unwrap().item_offsets = item_offsets;
    }

    fn get_name_alignment(&self) -> Align {
        if self
            .align_names_right
            .unwrap_or_else(|| self.config.layout.align_names_right())
        {
            Align::RIGHT
        } else {
            Align::LEFT
        }
    }

    fn draw_variable(
        &self,
        msgs: &mut Vec<Message>,
        vidx: usize,
        name: WidgetText,
        field: FieldRef,
        item_offsets: &mut Vec<ItemDrawingInfo>,
        info: &VariableInfo,
        ui: &mut egui::Ui,
    ) {
        let draw_label = |ui: &mut egui::Ui| {
            let mut variable_label = ui
                .selectable_label(self.item_is_selected(vidx), name)
                .context_menu(|ui| {
                    self.item_context_menu(Some(&field), msgs, ui, vidx);
                });

            if self
                .show_variable_tooltip
                .unwrap_or(self.config.layout.show_tooltip())
            {
                let tooltip = if let Some(waves) = &self.waves {
                    if field.field.is_empty() {
                        variable_tooltip_text(waves, &field.root)
                    } else {
                        "From translator".to_string()
                    }
                } else {
                    "No VCD loaded".to_string()
                };
                variable_label = variable_label.on_hover_text(tooltip);
            }

            if variable_label.clicked() {
                if self
                    .waves
                    .as_ref()
                    .is_some_and(|w| w.focused_item.is_some_and(|f| f == vidx))
                {
                    msgs.push(Message::UnfocusItem);
                } else {
                    msgs.push(Message::FocusItem(vidx));
                }
            }
            variable_label
        };

        match info {
            VariableInfo::Compound { subfields } => {
                let response = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    egui::Id::new(&field),
                    false,
                )
                .show_header(ui, |ui| {
                    ui.with_layout(
                        Layout::top_down(Align::LEFT).with_cross_justify(true),
                        draw_label,
                    );
                })
                .body(|ui| {
                    for (name, info) in subfields {
                        let mut new_path = field.clone();
                        new_path.field.push(name.clone());
                        self.draw_variable(
                            msgs,
                            vidx,
                            WidgetText::RichText(RichText::new(name)),
                            new_path,
                            item_offsets,
                            info,
                            ui,
                        );
                    }
                });

                item_offsets.push(ItemDrawingInfo::Variable(VariableDrawingInfo {
                    field_ref: field.clone(),
                    item_list_idx: vidx,
                    top: response.0.rect.top(),
                    bottom: response.0.rect.bottom(),
                }));
            }
            VariableInfo::Bool
            | VariableInfo::Bits
            | VariableInfo::Clock
            | VariableInfo::String
            | VariableInfo::Real => {
                let label = draw_label(ui);
                item_offsets.push(ItemDrawingInfo::Variable(VariableDrawingInfo {
                    field_ref: field.clone(),
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
        }
    }

    fn draw_plain_item(
        &self,
        msgs: &mut Vec<Message>,
        vidx: usize,
        displayed_item: &DisplayedItem,
        item_offsets: &mut Vec<ItemDrawingInfo>,
        ui: &mut egui::Ui,
        draw_alpha: bool,
    ) {
        let mut draw_label = |ui: &mut egui::Ui| {
            let style = Style::default();
            let mut layout_job = LayoutJob::default();
            self.add_alpha_id(draw_alpha, vidx, &style, &mut layout_job, Align::LEFT);

            let text_color = if let Some(color) = &displayed_item.color() {
                self.config
                    .theme
                    .colors
                    .get(color)
                    .unwrap_or(&self.config.theme.foreground)
            } else {
                &self.config.theme.foreground
            };
            displayed_item.add_to_layout_job(text_color, None, &style, &mut layout_job);
            self.add_alpha_id(draw_alpha, vidx, &style, &mut layout_job, Align::RIGHT);
            let item_label = ui
                .selectable_label(
                    self.item_is_selected(vidx),
                    WidgetText::LayoutJob(layout_job),
                )
                .context_menu(|ui| {
                    self.item_context_menu(None, msgs, ui, vidx);
                });
            if item_label.clicked() {
                msgs.push(Message::FocusItem(vidx))
            }
            item_label
        };

        let label = draw_label(ui);
        match displayed_item {
            DisplayedItem::Divider(_) => {
                item_offsets.push(ItemDrawingInfo::Divider(DividerDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }))
            }
            DisplayedItem::Cursor(cursor) => {
                item_offsets.push(ItemDrawingInfo::Cursor(CursorDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                    idx: cursor.idx,
                }))
            }
            DisplayedItem::TimeLine(_) => {
                item_offsets.push(ItemDrawingInfo::TimeLine(TimeLineDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }))
            }
            &DisplayedItem::Variable(_) => {}
            &DisplayedItem::Placeholder(_) => {}
        }
    }

    fn add_alpha_id(
        &self,
        draw_alpha: bool,
        vidx: usize,
        style: &Style,
        mut layout_job: &mut LayoutJob,
        alignment: Align,
    ) {
        if draw_alpha && alignment == self.get_name_alignment() {
            let alpha_id = uint_idx_to_alpha_idx(
                vidx,
                self.waves
                    .as_ref()
                    .map_or(0, |waves| waves.displayed_items.len()),
            );
            let text = egui::RichText::new(alpha_id)
                .background_color(self.config.theme.accent_info.background)
                .monospace()
                .color(self.config.theme.accent_info.foreground);
            if alignment == Align::LEFT {
                text.append_to(
                    &mut layout_job,
                    style,
                    FontSelection::Default,
                    Align::Center,
                );
                egui::RichText::new(" ").append_to(
                    &mut layout_job,
                    style,
                    FontSelection::Default,
                    Align::Center,
                );
            } else {
                egui::RichText::new(" ").append_to(
                    &mut layout_job,
                    style,
                    FontSelection::Default,
                    Align::Center,
                );
                text.append_to(
                    &mut layout_job,
                    style,
                    FontSelection::Default,
                    Align::Center,
                );
            }
        }
    }

    fn item_is_selected(&self, vidx: usize) -> bool {
        if let Some(waves) = &self.waves {
            waves.focused_item == Some(vidx)
        } else {
            false
        }
    }

    fn draw_var_values(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
        let Some(waves) = &self.waves else { return };
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        let container_rect = Rect::from_min_size(Pos2::ZERO, response.rect.size());
        let to_screen = RectTransform::from_to(container_rect, response.rect);
        let cfg = DrawConfig::new(response.rect.size().y);
        let frame_width = response.rect.width();

        let ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            // This 0.5 is very odd, but it fixes the lines we draw being smushed out across two
            // pixels, resulting in dimmer colors https://github.com/emilk/egui/issues/1322
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y) + Vec2::new(0.5, 0.5)),
            theme: &self.config.theme,
        };

        let gap = self.get_item_gap(&waves.item_offsets, &ctx);
        let yzero = to_screen.transform_pos(Pos2::ZERO).y;
        let ucursor = waves.cursor.as_ref().and_then(|u| u.to_biguint());
        ui.allocate_ui_at_rect(response.rect, |ui| {
            let text_style = TextStyle::Monospace;
            ui.style_mut().override_text_style = Some(text_style);
            for (vidx, drawing_info) in waves
                .item_offsets
                .iter()
                .sorted_by_key(|o| o.top() as i32)
                .enumerate()
            {
                let next_y = ui.cursor().top();
                // In order to align the text in this view with the variable tree,
                // we need to keep track of how far away from the expected offset we are,
                // and compensate for it
                if next_y < drawing_info.top() {
                    ui.add_space(drawing_info.top() - next_y);
                }

                let y_offset = drawing_info.top() - yzero;

                self.draw_background(vidx, waves, drawing_info, y_offset, &ctx, gap, frame_width);
                match drawing_info {
                    ItemDrawingInfo::Variable(drawing_info) => {
                        if ucursor.as_ref().is_none() {
                            ui.label("");
                            continue;
                        }

                        let translator = waves
                            .variable_translator(&drawing_info.field_ref, &self.sys.translators);

                        let variable = &drawing_info.field_ref.root;
                        let meta = waves.inner.variable_meta(variable);
                        let translation_result = waves
                            .inner
                            .query_variable(variable, ucursor.as_ref().unwrap())
                            .ok()
                            .and_then(|q| q.current)
                            .map(|(_time, value)| {
                                meta.and_then(|meta| translator.translate(&meta, &value))
                            });

                        if let Some(Ok(s)) = translation_result {
                            let subfields = s
                                .flatten(
                                    FieldRef::without_fields(drawing_info.field_ref.root.clone()),
                                    &waves.variable_format,
                                    &self.sys.translators,
                                )
                                .as_fields();

                            let subfield = subfields
                                .iter()
                                .find(|res| res.names == drawing_info.field_ref.field);

                            if let Some(SubFieldFlatTranslationResult {
                                names: _,
                                value: Some(TranslatedValue { value: v, kind: _ }),
                            }) = subfield
                            {
                                ui.label(v).context_menu(|ui| {
                                    self.item_context_menu(
                                        Some(&FieldRef::without_fields(variable.clone())),
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
                    ItemDrawingInfo::Divider(_) => {
                        ui.label("");
                    }
                    ItemDrawingInfo::Cursor(numbered_cursor) => {
                        if let Some(cursor) = &waves.cursor {
                            let delta = time_string(
                                &(waves.numbered_cursor_time(numbered_cursor.idx) - cursor),
                                &waves.inner.metadata().timescale,
                                &self.wanted_timeunit,
                                &self.get_time_format(),
                            );

                            ui.label(format!("Δ: {delta}",)).context_menu(|ui| {
                                self.item_context_menu(None, msgs, ui, vidx);
                            });
                        } else {
                            ui.label("");
                        }
                    }
                    ItemDrawingInfo::TimeLine(_) => {
                        ui.label("");
                    }
                }
            }
        });
    }

    pub fn get_item_gap(
        &self,
        item_offsets: &Vec<ItemDrawingInfo>,
        ctx: &DrawingContext<'_>,
    ) -> f32 {
        if item_offsets.len() >= 2.max(self.config.theme.alt_frequency) {
            // Assume that first variable has standard height (for now)
            (item_offsets.get(1).unwrap().top()
                - item_offsets.get(0).unwrap().top()
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
            .get(drawing_info.item_list_idx())
            .and_then(|variable| variable.background_color())
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

    pub fn draw_log_window(&self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        egui::Window::new("Logs")
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .show(ctx, |ui| {
                ui.style_mut().wrap = Some(false);

                ScrollArea::new([true, false]).show(ui, |ui| {
                    TableBuilder::new(ui)
                        .column(Column::auto().resizable(true))
                        .column(Column::remainder())
                        .vscroll(true)
                        .stick_to_bottom(true)
                        .header(20.0, |mut header| {
                            header.col(|ui| {
                                ui.heading("Level");
                            });
                            header.col(|ui| {
                                ui.heading("Message");
                            });
                        })
                        .body(|body| {
                            let records = EGUI_LOGGER.records();
                            let heights = records
                                .iter()
                                .map(|record| {
                                    let height = record.msg.lines().count() as f32;

                                    height * 15.
                                })
                                .collect::<Vec<_>>();

                            body.heterogeneous_rows(heights.into_iter(), |mut row: TableRow| {
                                let record = &records[row.index()];
                                row.col(|ui| {
                                    let (color, text) = match record.level {
                                        log::Level::Error => (Color32::RED, "Error"),
                                        log::Level::Warn => (Color32::YELLOW, "Warn"),
                                        log::Level::Info => (Color32::GREEN, "Info"),
                                        log::Level::Debug => (Color32::BLUE, "Debug"),
                                        log::Level::Trace => (Color32::GRAY, "Trace"),
                                    };

                                    ui.colored_label(color, text);
                                });
                                row.col(|ui| {
                                    ui.label(RichText::new(record.msg.clone()).monospace());
                                });
                            });
                        })
                })
            });
        if !open {
            msgs.push(Message::SetLogsVisible(false))
        }
    }
}

fn variable_tooltip_text(wave: &WaveData, variable: &VariableRef) -> String {
    let meta = wave.inner.variable_meta(variable).ok();
    format!(
        "{}\nNum bits: {}\nType: {}",
        variable.full_path_string(),
        meta.as_ref()
            .and_then(|meta| meta.num_bits)
            .map(|num_bits| format!("{num_bits}"))
            .unwrap_or_else(|| "unknown".to_string()),
        meta.and_then(|meta| meta.variable_type)
            .map(|variable_type| format!("{variable_type}"))
            .unwrap_or_else(|| "unknown".to_string())
    )
}

fn scope_tooltip_text(wave: &WaveData, scope: &ScopeRef) -> String {
    let other = wave.inner.get_scope_tooltip_data(scope);
    if other.is_empty() {
        format!("{scope}")
    } else {
        format!("{scope}\n{other}")
    }
}
