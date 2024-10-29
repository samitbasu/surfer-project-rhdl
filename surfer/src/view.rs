use color_eyre::eyre::Context;
use ecolor::Color32;
#[cfg(not(target_arch = "wasm32"))]
use egui::ViewportCommand;
use egui::{Frame, Layout, Painter, RichText, ScrollArea, Sense, TextStyle, UiBuilder, WidgetText};
use egui_extras::{Column, TableBuilder};
use egui_remixicon::icons;
use emath::{Align, Pos2, Rect, RectTransform, Vec2};
use epaint::{
    text::{LayoutJob, TextWrapMode},
    Margin, Rounding, Stroke,
};
use fzcmd::expand_command;
use itertools::Itertools;
use log::{info, warn};

use num::BigUint;
use surfer_translation_types::{
    SubFieldFlatTranslationResult, TranslatedValue, VariableInfo, VariableType,
};

#[cfg(feature = "performance_plot")]
use crate::benchmark::NUM_PERF_SAMPLES;
use crate::data_container::VariableType as VarType;
use crate::displayed_item::{
    draw_rename_window, DisplayedFieldRef, DisplayedItem, DisplayedItemIndex, DisplayedItemRef,
};
use crate::help::{
    draw_about_window, draw_control_help_window, draw_license_window, draw_quickstart_help_window,
};
use crate::time::time_string;
use crate::transaction_container::{StreamScopeRef, TransactionStreamRef};
use crate::translation::TranslationResultExt;
use crate::util::uint_idx_to_alpha_idx;
use crate::variable_direction::VariableDirectionExt;
use crate::wave_container::{
    FieldRef, FieldRefExt, ScopeRef, ScopeRefExt, VariableRef, VariableRefExt,
};
use crate::wave_data::ScopeType;
use crate::wave_source::LoadOptions;
use crate::{command_prompt::get_parser, wave_container::WaveContainer};
use crate::{
    command_prompt::show_command_prompt, config::HierarchyStyle, hierarchy, wave_data::WaveData,
    Message, MoveDir, State,
};
use crate::{config::SurferTheme, wave_container::VariableMeta};
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
    pub fn new(canvas_height: f32, line_height: f32, text_size: f32) -> Self {
        Self {
            canvas_height,
            line_height,
            text_size,
            max_transition_width: 6,
        }
    }
}

#[derive(Debug)]
pub struct VariableDrawingInfo {
    pub field_ref: FieldRef,
    pub displayed_field_ref: DisplayedFieldRef,
    pub item_list_idx: DisplayedItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct DividerDrawingInfo {
    pub item_list_idx: DisplayedItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct MarkerDrawingInfo {
    pub item_list_idx: DisplayedItemIndex,
    pub top: f32,
    pub bottom: f32,
    pub idx: u8,
}

#[derive(Debug)]
pub struct TimeLineDrawingInfo {
    pub item_list_idx: DisplayedItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct StreamDrawingInfo {
    pub transaction_stream_ref: TransactionStreamRef,
    pub item_list_idx: DisplayedItemIndex,
    pub top: f32,
    pub bottom: f32,
}

pub enum ItemDrawingInfo {
    Variable(VariableDrawingInfo),
    Divider(DividerDrawingInfo),
    Marker(MarkerDrawingInfo),
    TimeLine(TimeLineDrawingInfo),
    Stream(StreamDrawingInfo),
}

impl ItemDrawingInfo {
    pub fn top(&self) -> f32 {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.top,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Stream(drawing_info) => drawing_info.top,
        }
    }
    pub fn bottom(&self) -> f32 {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Stream(drawing_info) => drawing_info.bottom,
        }
    }
    pub fn item_list_idx(&self) -> usize {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.item_list_idx.0,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.item_list_idx.0,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.item_list_idx.0,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.item_list_idx.0,
            ItemDrawingInfo::Stream(drawing_info) => drawing_info.item_list_idx.0,
        }
    }
}

impl eframe::App for State {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(feature = "performance_plot")]
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

        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().start("draw");
        let mut msgs = self.draw(ctx, window_size);
        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().end("draw");

        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().start("update");
        let ui_zoom_factor = self.ui_zoom_factor();
        if ctx.zoom_factor() != ui_zoom_factor {
            ctx.set_zoom_factor(ui_zoom_factor);
        }

        self.sys.items_to_expand.borrow_mut().clear();

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
        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().end("update");

        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().start("handle_async_messages");
        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().end("handle_async_messages");

        self.handle_async_messages();
        self.handle_batch_commands();
        #[cfg(target_arch = "wasm32")]
        self.handle_wasm_external_messages();

        let viewport_is_moving = if let Some(waves) = &mut self.waves {
            let mut is_moving = false;
            for vp in &mut waves.viewports {
                if vp.is_moving() {
                    vp.move_viewport(ctx.input(|i| i.stable_dt));
                    is_moving = true;
                }
            }
            is_moving
        } else {
            false
        };

        if viewport_is_moving {
            self.invalidate_draw_commands();
            ctx.request_repaint();
        }

        // We can save some user battery life by not redrawing unless needed. At the moment,
        // we only need to continuously redraw to make surfer interactive during loading, otherwise
        // we'll let egui manage repainting. In practice
        if self.sys.continuous_redraw
            || self.sys.progress_tracker.is_some()
            || self.show_performance
        {
            ctx.request_repaint();
        }

        #[cfg(feature = "performance_plot")]
        if let Some(prev_cpu) = frame.info().cpu_usage {
            self.sys.rendering_cpu_times.push_back(prev_cpu);
            if self.sys.rendering_cpu_times.len() > NUM_PERF_SAMPLES {
                self.sys.rendering_cpu_times.pop_front();
            }
        }

        #[cfg(feature = "performance_plot")]
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

        if self.show_license {
            draw_license_window(ctx, &mut msgs);
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

        if let Some(dialog) = &self.show_reload_suggestion {
            self.draw_reload_waveform_dialog(ctx, dialog, &mut msgs);
        }

        if self.show_performance {
            #[cfg(feature = "performance_plot")]
            self.draw_performance_graph(ctx, &mut msgs);
        }

        if self.show_cursor_window {
            if let Some(waves) = &self.waves {
                self.draw_marker_window(waves, ctx, &mut msgs);
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

        if self.show_toolbar() {
            self.add_toolbar_panel(ctx, &mut msgs);
        }

        if self.show_url_entry {
            self.draw_load_url(ctx, &mut msgs);
        }

        if self.show_statusbar() {
            self.add_statusbar_panel(ctx, &self.waves, &mut msgs);
        }
        if let Some(waves) = &self.waves {
            if self.show_overview() && !waves.displayed_items_order.is_empty() {
                self.add_overview_panel(ctx, waves, &mut msgs);
            }
        }

        if self.show_hierarchy() {
            egui::SidePanel::left("variable select left panel")
                .default_width(300.)
                .width_range(100.0..=max_width)
                .frame(Frame {
                    fill: self.config.theme.primary_ui_color.background,
                    ..Default::default()
                })
                .show(ctx, |ui| {
                    self.sidepanel_width = Some(ui.clip_rect().width());
                    match self.config.layout.hierarchy_style {
                        HierarchyStyle::Separate => hierarchy::separate(self, ui, &mut msgs),
                        HierarchyStyle::Tree => hierarchy::tree(self, ui, &mut msgs),
                    }
                });
        }

        if self.sys.command_prompt.visible {
            show_command_prompt(self, ctx, window_size, &mut msgs);
            if let Some(new_idx) = self.sys.command_prompt.new_selection {
                self.sys.command_prompt.selected = new_idx;
                self.sys.command_prompt.new_selection = None;
            }
        }

        if self.waves.is_some() {
            let scroll_offset = self.waves.as_ref().unwrap().scroll_offset;
            if self.waves.as_ref().unwrap().any_displayed() {
                let draw_focus_ids = self.sys.command_prompt.visible
                    && expand_command(&self.sys.command_prompt_text.borrow(), get_parser(self))
                        .expanded
                        .starts_with("item_focus");
                if draw_focus_ids {
                    egui::SidePanel::left("focus id list")
                        .default_width(40.)
                        .width_range(40.0..=max_width)
                        .show(ctx, |ui| {
                            self.handle_pointer_in_ui(ui, &mut msgs);
                            let response = ScrollArea::both()
                                .vertical_scroll_offset(scroll_offset)
                                .show(ui, |ui| {
                                    self.draw_item_focus_list(ui);
                                });
                            self.waves.as_mut().unwrap().top_item_draw_offset =
                                response.inner_rect.min.y;
                            self.waves.as_mut().unwrap().total_height =
                                response.inner_rect.height();
                            if (scroll_offset - response.state.offset.y).abs() > 5. {
                                msgs.push(Message::SetScrollOffset(response.state.offset.y));
                            }
                        });
                }

                egui::SidePanel::left("variable list")
                    .default_width(200.)
                    .width_range(100.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        self.handle_pointer_in_ui(ui, &mut msgs);
                        let response = ScrollArea::both()
                            .vertical_scroll_offset(scroll_offset)
                            .show(ui, |ui| {
                                self.draw_item_list(&mut msgs, ui, ctx);
                            });
                        self.waves.as_mut().unwrap().top_item_draw_offset =
                            response.inner_rect.min.y;
                        self.waves.as_mut().unwrap().total_height = response.inner_rect.height();
                        if (scroll_offset - response.state.offset.y).abs() > 5. {
                            msgs.push(Message::SetScrollOffset(response.state.offset.y));
                        }
                    });

                if self.waves.as_ref().unwrap().focused_transaction.1.is_some() {
                    egui::SidePanel::right("Transaction Details")
                        .default_width(330.)
                        .width_range(10.0..=max_width)
                        .show(ctx, |ui| {
                            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                            self.handle_pointer_in_ui(ui, &mut msgs);
                            self.draw_focused_transaction_details(ui);
                        });
                }

                egui::SidePanel::left("variable values")
                    .frame(Frame {
                        inner_margin: Margin::ZERO,
                        outer_margin: Margin::ZERO,
                        ..Default::default()
                    })
                    .default_width(100.)
                    .width_range(10.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        self.handle_pointer_in_ui(ui, &mut msgs);
                        let response = ScrollArea::both()
                            .vertical_scroll_offset(scroll_offset)
                            .show(ui, |ui| self.draw_var_values(ui, &mut msgs));
                        if (scroll_offset - response.state.offset.y).abs() > 5. {
                            msgs.push(Message::SetScrollOffset(response.state.offset.y));
                        }
                    });
                let std_stroke = ctx.style().visuals.widgets.noninteractive.bg_stroke;
                ctx.style_mut(|style| {
                    style.visuals.widgets.noninteractive.bg_stroke = Stroke {
                        width: self.config.theme.viewport_separator.width,
                        color: self.config.theme.viewport_separator.color,
                    };
                });
                let number_of_viewports = self.waves.as_ref().unwrap().viewports.len();
                if number_of_viewports > 1 {
                    // Draw additional viewports
                    let max_width = ctx.available_rect().width();
                    let default_width = max_width / (number_of_viewports as f32);
                    for viewport_idx in 1..number_of_viewports {
                        egui::SidePanel::right(format! {"view port {viewport_idx}"})
                            .default_width(default_width)
                            .width_range(30.0..=max_width)
                            .frame(Frame {
                                inner_margin: Margin::ZERO,
                                outer_margin: Margin::ZERO,
                                ..Default::default()
                            })
                            .show(ctx, |ui| self.draw_items(&mut msgs, ui, viewport_idx));
                    }
                }

                egui::CentralPanel::default()
                    .frame(Frame {
                        inner_margin: Margin::ZERO,
                        outer_margin: Margin::ZERO,
                        ..Default::default()
                    })
                    .show(ctx, |ui| {
                        self.draw_items(&mut msgs, ui, 0);
                    });
                ctx.style_mut(|style| {
                    style.visuals.widgets.noninteractive.bg_stroke = std_stroke;
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
                .frame(Frame::none().fill(self.config.theme.canvas_colors.background))
                .show(ctx, |ui| {
                    ui.add_space(max_height * 0.1);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🏄 Surfer").monospace().size(24.));
                        ui.add_space(20.);
                        let layout = Layout::top_down(Align::LEFT);
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
                msgs.push(Message::FileDropped(file.clone()));
            });
        });

        // If some dialogs are open, skip decoding keypresses
        if !self.show_url_entry
            && self.rename_target.is_none()
            && self.show_reload_suggestion.is_none()
        {
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
                            msgs.push(Message::SetUrlEntryVisible(false));
                        }
                        if ui.button("Cancel").clicked() {
                            msgs.push(Message::SetUrlEntryVisible(false));
                        }
                    });
                });
            });
        if !open {
            msgs.push(Message::SetUrlEntryVisible(false));
        }
    }

    fn handle_pointer_in_ui(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
        if ui.ui_contains_pointer() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
            if scroll_delta.y > 0.0 {
                msgs.push(Message::InvalidateCount);
                msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
            } else if scroll_delta.y < 0.0 {
                msgs.push(Message::InvalidateCount);
                msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
            }
        }
    }

    pub fn draw_all_scopes(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        draw_variables: bool,
        ui: &mut egui::Ui,
        filter: &str,
    ) {
        for scope in wave.inner.root_scopes() {
            match scope {
                ScopeType::WaveScope(scope) => {
                    self.draw_selectable_child_or_orphan_scope(
                        msgs,
                        wave,
                        &scope,
                        draw_variables,
                        ui,
                        filter,
                    );
                }
                ScopeType::StreamScope(_) => {
                    self.draw_transaction_root(msgs, wave, ui);
                }
            }
        }
        if draw_variables {
            if let Some(wave_container) = wave.inner.as_waves() {
                let scope = ScopeRef::empty();
                let variables = wave_container.variables_in_scope(&scope);
                self.draw_variable_list(msgs, wave_container, ui, &variables, filter);
            }
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
            wave.active_scope == Some(ScopeType::WaveScope(scope.clone())),
            name,
        ));
        let _ = response.interact(egui::Sense::click_and_drag());
        response.drag_started().then(|| {
            msgs.push(Message::VariableDragStarted(
                self.waves.as_ref().unwrap().display_item_ref_counter.into(),
            ))
        });

        response.drag_stopped().then(|| {
            if ui.input(|i| i.pointer.hover_pos().unwrap_or_default().x)
                > self.sidepanel_width.unwrap_or_default()
            {
                let scope_t = ScopeType::WaveScope(scope.clone());
                let variables = self
                    .waves
                    .as_ref()
                    .unwrap()
                    .inner
                    .variables_in_scope(&scope_t)
                    .iter()
                    .filter_map(|var| match var {
                        VarType::Variable(var) => Some(var.clone()),
                        _ => None,
                    })
                    .collect_vec();

                msgs.push(Message::AddDraggedVariables(self.filtered_variables(
                    variables.as_slice(),
                    self.sys.variable_name_filter.borrow_mut().as_str(),
                )));
            }
        });
        if self.show_tooltip() {
            response = response.on_hover_text(scope_tooltip_text(wave, scope));
        }
        response
            .clicked()
            .then(|| msgs.push(Message::SetActiveScope(ScopeType::WaveScope(scope.clone()))));
    }

    fn draw_selectable_child_or_orphan_scope(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        scope: &ScopeRef,
        draw_variables: bool,
        ui: &mut egui::Ui,
        filter: &str,
    ) {
        let Some(child_scopes) = wave
            .inner
            .as_waves()
            .unwrap()
            .child_scopes(scope)
            .context("Failed to get child scopes")
            .map_err(|e| warn!("{e:#?}"))
            .ok()
        else {
            return;
        };

        let no_variables_in_scope = wave.inner.as_waves().unwrap().no_variables_in_scope(scope);
        if child_scopes.is_empty() && no_variables_in_scope && !self.show_empty_scopes() {
            return;
        }
        if child_scopes.is_empty() && (!draw_variables || no_variables_in_scope) {
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
            .body(|ui| {
                if draw_variables || self.show_parameters_in_scopes() {
                    let wave_container = wave.inner.as_waves().unwrap();
                    let all_variables = wave_container.variables_in_scope(scope);
                    let parameters = all_variables
                        .iter()
                        .filter(|var| {
                            let meta = wave_container.variable_meta(var).ok();
                            meta.unwrap().variable_type == Some(VariableType::VCDParameter)
                        })
                        .cloned()
                        .collect_vec();
                    if !parameters.is_empty() {
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            egui::Id::new(&parameters),
                            false,
                        )
                        .show_header(ui, |ui| {
                            ui.with_layout(
                                Layout::top_down(Align::LEFT).with_cross_justify(true),
                                |ui| {
                                    ui.label("Parameters");
                                },
                            );
                        })
                        .body(|ui| {
                            self.draw_variable_list(msgs, wave_container, ui, &parameters, filter);
                        });
                    }
                }
                self.draw_root_scope_view(msgs, wave, scope, draw_variables, ui, filter);
                if draw_variables {
                    let wave_container = wave.inner.as_waves().unwrap();
                    let all_variables = wave_container.variables_in_scope(scope);
                    let variables = all_variables
                        .iter()
                        .filter(|var| {
                            let meta = wave_container.variable_meta(var).ok();
                            meta.unwrap().variable_type != Some(VariableType::VCDParameter)
                        })
                        .cloned()
                        .collect_vec();
                    self.draw_variable_list(msgs, wave_container, ui, &variables, filter);
                }
            });
        }
    }

    fn draw_root_scope_view(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        root_scope: &ScopeRef,
        draw_variables: bool,
        ui: &mut egui::Ui,
        filter: &str,
    ) {
        let Some(child_scopes) = wave
            .inner
            .as_waves()
            .unwrap()
            .child_scopes(root_scope)
            .context("Failed to get child scopes")
            .map_err(|e| warn!("{e:#?}"))
            .ok()
        else {
            return;
        };

        for child_scope in child_scopes {
            self.draw_selectable_child_or_orphan_scope(
                msgs,
                wave,
                &child_scope,
                draw_variables,
                ui,
                filter,
            );
        }
    }

    pub fn draw_variable_list(
        &self,
        msgs: &mut Vec<Message>,
        wave_container: &WaveContainer,
        ui: &mut egui::Ui,
        variables: &[VariableRef],
        filter: &str,
    ) {
        for variable in self.filtered_variables(variables, filter) {
            let meta = wave_container.variable_meta(&variable).ok();
            let index = meta
                .as_ref()
                .and_then(|meta| meta.index.clone())
                .map(|index| format!(" {index}"))
                .unwrap_or_default();

            let direction = if self.show_variable_direction() {
                meta.as_ref()
                    .and_then(|meta| meta.direction)
                    .map(|direction| {
                        format!(
                            "{} ",
                            // Icon based on direction
                            direction.get_icon().unwrap_or_else(|| {
                                if meta.as_ref().is_some_and(|meta| {
                                    meta.variable_type == Some(VariableType::VCDParameter)
                                }) {
                                    // If parameter
                                    icons::MAP_PIN_2_LINE
                                } else {
                                    // Align other items (can be improved)
                                    "    "
                                }
                            })
                        )
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };

            let value = if meta
                .as_ref()
                .is_some_and(|meta| meta.variable_type == Some(VariableType::VCDParameter))
            {
                let res = wave_container
                    .query_variable(&variable, &BigUint::ZERO)
                    .ok();
                res.and_then(|o| o.and_then(|q| q.current.map(|v| format!(": {}", v.1))))
                    .unwrap_or_else(|| ": Undefined".to_string())
            } else {
                String::new()
            };

            let variable_name = format!("{direction}{}{index}{value}", variable.name.clone());
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    let mut response = ui.add(egui::SelectableLabel::new(false, variable_name));
                    let _ = response.interact(egui::Sense::click_and_drag());

                    if self.show_tooltip() {
                        // Should be possible to reuse the meta from above?
                        let meta = wave_container.variable_meta(&variable).ok();
                        response = response.on_hover_text(variable_tooltip_text(&meta, &variable));
                    }
                    response.drag_started().then(|| {
                        msgs.push(Message::VariableDragStarted(
                            self.waves.as_ref().unwrap().display_item_ref_counter.into(),
                        ))
                    });
                    response.drag_stopped().then(|| {
                        if ui.input(|i| i.pointer.hover_pos().unwrap_or_default().x)
                            > self.sidepanel_width.unwrap_or_default()
                        {
                            msgs.push(Message::AddDraggedVariables(vec![variable.clone()]));
                        }
                    });
                    response
                        .clicked()
                        .then(|| msgs.push(Message::AddVariables(vec![variable.clone()])));
                },
            );
        }
    }

    fn draw_item_focus_list(&self, ui: &mut egui::Ui) {
        let alignment = self.get_name_alignment();
        ui.with_layout(
            Layout::top_down(alignment).with_cross_justify(false),
            |ui| {
                for (vidx, _) in self
                    .waves
                    .as_ref()
                    .unwrap()
                    .displayed_items_order
                    .iter()
                    .enumerate()
                {
                    let vidx = vidx.into();
                    ui.scope(|ui| {
                        ui.style_mut().visuals.selection.bg_fill =
                            self.config.theme.accent_warn.background;
                        ui.style_mut().visuals.override_text_color =
                            Some(self.config.theme.accent_warn.foreground);
                        let _ = ui.selectable_label(true, self.get_alpha_focus_id(vidx));
                    });
                }
            },
        );
    }

    fn draw_item_list(&mut self, msgs: &mut Vec<Message>, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut item_offsets = Vec::new();

        let alignment = self.get_name_alignment();
        ui.with_layout(Layout::top_down(alignment).with_cross_justify(true), |ui| {
            for (vidx, displayed_item_id) in self
                .waves
                .as_ref()
                .unwrap()
                .displayed_items_order
                .iter()
                .enumerate()
            {
                let vidx = vidx.into();
                if let Some(displayed_item) = self
                    .waves
                    .as_ref()
                    .unwrap()
                    .displayed_items
                    .get(displayed_item_id)
                {
                    let item_rect = match displayed_item {
                        DisplayedItem::Variable(displayed_variable) => {
                            let levels_to_force_expand =
                                self.sys.items_to_expand.borrow().iter().find_map(
                                    |(id, levels)| {
                                        if displayed_item_id == id {
                                            Some(*levels)
                                        } else {
                                            None
                                        }
                                    },
                                );

                            self.draw_variable(
                                msgs,
                                vidx,
                                displayed_item,
                                *displayed_item_id,
                                FieldRef::without_fields(displayed_variable.variable_ref.clone()),
                                &mut item_offsets,
                                &displayed_variable.info,
                                ui,
                                ctx,
                                levels_to_force_expand,
                            )
                        }
                        DisplayedItem::Divider(_) => self.draw_plain_item(
                            msgs,
                            vidx,
                            *displayed_item_id,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                        ),
                        DisplayedItem::Marker(_) => self.draw_plain_item(
                            msgs,
                            vidx,
                            *displayed_item_id,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                        ),
                        DisplayedItem::Placeholder(_) => self.draw_plain_item(
                            msgs,
                            vidx,
                            *displayed_item_id,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                        ),
                        DisplayedItem::TimeLine(_) => self.draw_plain_item(
                            msgs,
                            vidx,
                            *displayed_item_id,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                        ),
                        DisplayedItem::Stream(_) => self.draw_plain_item(
                            msgs,
                            vidx,
                            *displayed_item_id,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                        ),
                    };
                    self.draw_drag_target(
                        msgs,
                        vidx,
                        item_rect,
                        ui,
                        vidx.0 == self.waves.as_ref().unwrap().displayed_items_order.len() - 1,
                    );
                };
            }
        });

        self.waves.as_mut().unwrap().drawing_infos = item_offsets;
    }

    fn draw_transaction_root(
        &self,
        msgs: &mut Vec<Message>,
        streams: &WaveData,
        ui: &mut egui::Ui,
    ) {
        egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            egui::Id::from("Streams"),
            false,
        )
        .show_header(ui, |ui| {
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    let root_name = String::from("tr");
                    let response = ui.add(egui::SelectableLabel::new(
                        streams.active_scope == Some(ScopeType::StreamScope(StreamScopeRef::Root)),
                        root_name,
                    ));

                    response.clicked().then(|| {
                        msgs.push(Message::SetActiveScope(ScopeType::StreamScope(
                            StreamScopeRef::Root,
                        )));
                    });
                },
            );
        })
        .body(|ui| {
            for (id, stream) in &streams.inner.as_transactions().unwrap().inner.tx_streams {
                let name = stream.name.clone();
                let response = ui.add(egui::SelectableLabel::new(
                    streams.active_scope.as_ref().is_some_and(|s| {
                        if let ScopeType::StreamScope(StreamScopeRef::Stream(scope_stream)) = s {
                            scope_stream.stream_id == *id
                        } else {
                            false
                        }
                    }),
                    name.clone(),
                ));

                response.clicked().then(|| {
                    msgs.push(Message::SetActiveScope(ScopeType::StreamScope(
                        StreamScopeRef::Stream(TransactionStreamRef::new_stream(*id, name)),
                    )));
                });
            }
        });
    }

    pub fn draw_transaction_variable_list(
        &self,
        msgs: &mut Vec<Message>,
        streams: &WaveData,
        ui: &mut egui::Ui,
        active_stream: &StreamScopeRef,
    ) {
        let inner = streams.inner.as_transactions().unwrap();
        match active_stream {
            StreamScopeRef::Root => {
                for stream in inner.get_streams() {
                    ui.with_layout(
                        Layout::top_down(Align::LEFT).with_cross_justify(true),
                        |ui| {
                            let response =
                                ui.add(egui::SelectableLabel::new(false, stream.name.clone()));

                            response.clicked().then(|| {
                                msgs.push(Message::AddStreamOrGenerator(
                                    TransactionStreamRef::new_stream(
                                        stream.id,
                                        stream.name.clone(),
                                    ),
                                ));
                            });
                        },
                    );
                }
            }
            StreamScopeRef::Stream(stream_ref) => {
                for gen_id in &inner.get_stream(stream_ref.stream_id).unwrap().generators {
                    let gen_name = inner.get_generator(*gen_id).unwrap().name.clone();
                    ui.with_layout(
                        Layout::top_down(Align::LEFT).with_cross_justify(true),
                        |ui| {
                            let response = ui.add(egui::SelectableLabel::new(false, &gen_name));

                            response.clicked().then(|| {
                                msgs.push(Message::AddStreamOrGenerator(
                                    TransactionStreamRef::new_gen(
                                        stream_ref.stream_id,
                                        *gen_id,
                                        gen_name,
                                    ),
                                ));
                            });
                        },
                    );
                }
            }
            StreamScopeRef::Empty(_) => {}
        }
    }
    fn draw_focused_transaction_details(&self, ui: &mut egui::Ui) {
        ui.with_layout(
            Layout::top_down(Align::LEFT).with_cross_justify(true),
            |ui| {
                ui.label("Focused Transaction Details");
                let column_width = ui.available_width() / 2.;
                TableBuilder::new(ui)
                    .column(Column::exact(column_width))
                    .column(Column::auto())
                    .header(20.0, |mut header| {
                        header.col(|ui| {
                            ui.heading("Properties");
                        });
                    })
                    .body(|mut body| {
                        let focused_transaction = self
                            .waves
                            .as_ref()
                            .unwrap()
                            .focused_transaction
                            .1
                            .as_ref()
                            .unwrap();
                        let row_height = 15.;
                        body.row(row_height, |mut row| {
                            row.col(|ui| {
                                ui.label("Transaction ID");
                            });
                            row.col(|ui| {
                                ui.label(format!("{}", focused_transaction.get_tx_id()));
                            });
                        });
                        body.row(row_height, |mut row| {
                            row.col(|ui| {
                                ui.label("Type");
                            });
                            row.col(|ui| {
                                let gen = self
                                    .waves
                                    .as_ref()
                                    .unwrap()
                                    .inner
                                    .as_transactions()
                                    .unwrap()
                                    .get_generator(focused_transaction.get_gen_id())
                                    .unwrap();
                                ui.label(format!("{}", gen.name));
                            });
                        });
                        body.row(row_height, |mut row| {
                            row.col(|ui| {
                                ui.label("Start Time");
                            });
                            row.col(|ui| {
                                ui.label(format!("{}", focused_transaction.get_start_time()));
                            });
                        });
                        body.row(row_height, |mut row| {
                            row.col(|ui| {
                                ui.label("End Time");
                            });
                            row.col(|ui| {
                                ui.label(format!("{}", focused_transaction.get_end_time()));
                            });
                        });
                        body.row(row_height + 5., |mut row| {
                            row.col(|ui| {
                                ui.heading("Attributes");
                            });
                        });

                        body.row(row_height + 3., |mut row| {
                            row.col(|ui| {
                                ui.label(RichText::new("Name").size(15.));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new("Value").size(15.));
                            });
                        });

                        for attr in &focused_transaction.attributes {
                            body.row(row_height, |mut row| {
                                row.col(|ui| {
                                    ui.label(format!("{}", attr.name));
                                });
                                row.col(|ui| {
                                    ui.label(format!("{}", attr.value()));
                                });
                            });
                        }

                        if !focused_transaction.inc_relations.is_empty() {
                            body.row(row_height + 5., |mut row| {
                                row.col(|ui| {
                                    ui.heading("Incoming Relations");
                                });
                            });

                            body.row(row_height + 3., |mut row| {
                                row.col(|ui| {
                                    ui.label(RichText::new("Source Tx").size(15.));
                                });
                                row.col(|ui| {
                                    ui.label(RichText::new("Sink Tx").size(15.));
                                });
                            });

                            for rel in &focused_transaction.inc_relations {
                                body.row(row_height, |mut row| {
                                    row.col(|ui| {
                                        ui.label(format!("{}", rel.source_tx_id));
                                    });
                                    row.col(|ui| {
                                        ui.label(format!("{}", rel.sink_tx_id));
                                    });
                                });
                            }
                        }

                        if !focused_transaction.out_relations.is_empty() {
                            body.row(row_height + 5., |mut row| {
                                row.col(|ui| {
                                    ui.heading("Outgoing Relations");
                                });
                            });

                            body.row(row_height + 3., |mut row| {
                                row.col(|ui| {
                                    ui.label(RichText::new("Source Tx").size(15.));
                                });
                                row.col(|ui| {
                                    ui.label(RichText::new("Sink Tx").size(15.));
                                });
                            });

                            for rel in &focused_transaction.out_relations {
                                body.row(row_height, |mut row| {
                                    row.col(|ui| {
                                        ui.label(format!("{}", rel.source_tx_id));
                                    });
                                    row.col(|ui| {
                                        ui.label(format!("{}", rel.sink_tx_id));
                                    });
                                });
                            }
                        }
                    });
            },
        );
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

    fn draw_drag_source(
        &self,
        msgs: &mut Vec<Message>,
        vidx: DisplayedItemIndex,
        item_response: &egui::Response,
    ) {
        if item_response.dragged_by(egui::PointerButton::Primary)
            && item_response.drag_delta().length() > self.config.theme.drag_threshold
        {
            msgs.push(Message::VariableDragStarted(vidx));
        }

        if item_response.drag_stopped()
            && self
                .drag_source_idx
                .is_some_and(|source_idx| source_idx == vidx)
        {
            msgs.push(Message::VariableDragFinished);
        }
    }

    fn draw_variable(
        &self,
        msgs: &mut Vec<Message>,
        vidx: DisplayedItemIndex,
        displayed_item: &DisplayedItem,
        displayed_id: DisplayedItemRef,
        field: FieldRef,
        drawing_infos: &mut Vec<ItemDrawingInfo>,
        info: &VariableInfo,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        levels_to_force_expand: Option<usize>,
    ) -> Rect {
        let mut draw_label = |ui: &mut egui::Ui| {
            let style = ui.style_mut();
            let text_color: Color32;
            if self.item_is_focused(vidx) {
                style.visuals.selection.bg_fill = self.config.theme.accent_info.background;
                text_color = self.config.theme.accent_info.foreground;
            } else if self.item_is_selected(displayed_id) {
                style.visuals.selection.bg_fill =
                    self.config.theme.selected_elements_colors.background;
                text_color = self.config.theme.selected_elements_colors.foreground;
            } else {
                style.visuals.selection.bg_fill = self.config.theme.primary_ui_color.background;
                text_color = self.config.theme.primary_ui_color.foreground;
            }

            let mut layout_job = LayoutJob::default();
            displayed_item.add_to_layout_job(&text_color, style, &mut layout_job, &self.config);

            let mut variable_label = ui
                .selectable_label(
                    self.item_is_selected(displayed_id) || self.item_is_focused(vidx),
                    WidgetText::LayoutJob(layout_job),
                )
                .interact(Sense::drag());
            variable_label.context_menu(|ui| {
                self.item_context_menu(Some(&field), msgs, ui, vidx);
            });

            if self.show_tooltip() {
                let tooltip = if let Some(waves) = &self.waves {
                    if field.field.is_empty() {
                        let wave_container = waves.inner.as_waves().unwrap();
                        let meta = wave_container.variable_meta(&field.root).ok();
                        variable_tooltip_text(&meta, &field.root)
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
                    let modifiers = ctx.input(|i| i.modifiers);
                    if modifiers.ctrl {
                        msgs.push(Message::ToggleItemSelected(Some(vidx)));
                    } else if modifiers.shift {
                        msgs.push(Message::Batch(vec![
                            Message::ItemSelectionClear,
                            Message::ItemSelectRange(vidx),
                        ]));
                    } else {
                        msgs.push(Message::Batch(vec![
                            Message::ItemSelectionClear,
                            Message::FocusItem(vidx),
                        ]));
                    }
                }
            }

            variable_label
        };

        let displayed_field_ref = DisplayedFieldRef {
            item: displayed_id,
            field: field.field.clone(),
        };
        match info {
            VariableInfo::Compound { subfields } => {
                let mut header = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    egui::Id::new(&field),
                    false,
                );

                if let Some(level) = levels_to_force_expand {
                    header.set_open(level > 0);
                }

                let response = header
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
                                displayed_item,
                                displayed_id,
                                new_path,
                                drawing_infos,
                                info,
                                ui,
                                ctx,
                                levels_to_force_expand.map(|l| l.saturating_sub(1)),
                            );
                        }
                    });

                drawing_infos.push(ItemDrawingInfo::Variable(VariableDrawingInfo {
                    displayed_field_ref,
                    field_ref: field.clone(),
                    item_list_idx: vidx,
                    top: response.0.rect.top(),
                    bottom: response.0.rect.bottom(),
                }));
                response.0.rect
            }
            VariableInfo::Bool
            | VariableInfo::Bits
            | VariableInfo::Clock
            | VariableInfo::String
            | VariableInfo::Real => {
                let label = draw_label(ui);
                self.draw_drag_source(msgs, vidx, &label);
                drawing_infos.push(ItemDrawingInfo::Variable(VariableDrawingInfo {
                    displayed_field_ref,
                    field_ref: field.clone(),
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
                label.rect
            }
        }
    }

    fn draw_drag_target(
        &self,
        msgs: &mut Vec<Message>,
        vidx: DisplayedItemIndex,
        item_rect: Rect,
        ui: &mut egui::Ui,
        last: bool,
    ) {
        // Add default margin as it was removed when creating the frame
        let rect_with_margin = Rect {
            min: item_rect.min - ui.spacing().item_spacing / 2f32,
            max: item_rect.max + ui.spacing().item_spacing / 2f32,
        };

        let vertical_translation_up = Vec2 {
            x: 0f32,
            y: -rect_with_margin.height() / 2f32,
        };

        let before_rect = Rect {
            min: ui
                .painter()
                .round_pos_to_pixels(rect_with_margin.left_top()),
            max: ui
                .painter()
                .round_pos_to_pixels(rect_with_margin.right_bottom() + vertical_translation_up),
        };

        let expanded_after_rect = if last {
            ui.max_rect().max
        } else {
            rect_with_margin.right_bottom()
        };

        let after_rect = Rect {
            min: ui.painter().round_pos_to_pixels(before_rect.left_bottom()),
            max: ui.painter().round_pos_to_pixels(expanded_after_rect),
        };

        let half_line_width = Vec2 {
            x: 0f32,
            y: self.config.theme.linewidth / 2f32,
        };

        let vidx = vidx.0;
        if self.drag_started {
            if let Some(DisplayedItemIndex(source_idx)) = self.drag_source_idx {
                let target_idx = if ui.rect_contains_pointer(before_rect) {
                    ui.painter().rect_filled(
                        Rect {
                            min: rect_with_margin.left_top() - half_line_width,
                            max: rect_with_margin.right_top() + half_line_width,
                        },
                        egui::Rounding::ZERO,
                        self.config.theme.drag_hint_color,
                    );
                    if vidx > source_idx {
                        vidx - 1
                    } else {
                        vidx
                    }
                } else if ui.rect_contains_pointer(after_rect) {
                    ui.painter().rect_filled(
                        Rect {
                            min: rect_with_margin.left_bottom() - half_line_width,
                            max: rect_with_margin.right_bottom() + half_line_width,
                        },
                        egui::Rounding::ZERO,
                        self.config.theme.drag_hint_color,
                    );
                    if vidx < source_idx {
                        vidx + 1
                    } else {
                        vidx
                    }
                } else {
                    source_idx
                };

                if source_idx != target_idx {
                    msgs.push(Message::VariableDragTargetChanged(target_idx.into()));
                }
            }
        }
    }

    fn draw_plain_item(
        &self,
        msgs: &mut Vec<Message>,
        vidx: DisplayedItemIndex,
        displayed_id: DisplayedItemRef,
        displayed_item: &DisplayedItem,
        drawing_infos: &mut Vec<ItemDrawingInfo>,
        ui: &mut egui::Ui,
    ) -> Rect {
        let mut draw_label = |ui: &mut egui::Ui| {
            let style = ui.style_mut();
            let mut layout_job = LayoutJob::default();
            let text_color: Color32;

            if self.item_is_focused(vidx) {
                style.visuals.selection.bg_fill = self.config.theme.accent_info.background;
                text_color = self.config.theme.accent_info.foreground;
            } else if self.item_is_selected(displayed_id) {
                style.visuals.selection.bg_fill =
                    self.config.theme.selected_elements_colors.background;
                text_color = self.config.theme.selected_elements_colors.foreground;
            } else {
                style.visuals.selection.bg_fill = self.config.theme.primary_ui_color.background;
                text_color = *self.get_item_text_color(displayed_item);
            }

            displayed_item.add_to_layout_job(&text_color, style, &mut layout_job, &self.config);

            let item_label = ui
                .selectable_label(
                    self.item_is_selected(displayed_id) || self.item_is_focused(vidx),
                    WidgetText::LayoutJob(layout_job),
                )
                .interact(Sense::drag());
            item_label.context_menu(|ui| {
                self.item_context_menu(None, msgs, ui, vidx);
            });
            if item_label.clicked() {
                msgs.push(Message::FocusItem(vidx));
            }
            item_label
        };

        let label = draw_label(ui);
        self.draw_drag_source(msgs, vidx, &label);
        match displayed_item {
            DisplayedItem::Divider(_) => {
                drawing_infos.push(ItemDrawingInfo::Divider(DividerDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            DisplayedItem::Marker(cursor) => {
                drawing_infos.push(ItemDrawingInfo::Marker(MarkerDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                    idx: cursor.idx,
                }));
            }
            DisplayedItem::TimeLine(_) => {
                drawing_infos.push(ItemDrawingInfo::TimeLine(TimeLineDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            DisplayedItem::Stream(stream) => {
                drawing_infos.push(ItemDrawingInfo::Stream(StreamDrawingInfo {
                    transaction_stream_ref: stream.transaction_stream_ref.clone(),
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            &DisplayedItem::Variable(_) => {}
            &DisplayedItem::Placeholder(_) => {}
        }
        label.rect
    }

    fn get_alpha_focus_id(&self, vidx: DisplayedItemIndex) -> RichText {
        let alpha_id = uint_idx_to_alpha_idx(
            vidx,
            self.waves
                .as_ref()
                .map_or(0, |waves| waves.displayed_items.len()),
        );

        RichText::new(alpha_id).monospace()
    }

    fn item_is_focused(&self, vidx: DisplayedItemIndex) -> bool {
        if let Some(waves) = &self.waves {
            waves.focused_item == Some(vidx)
        } else {
            false
        }
    }

    fn item_is_selected(&self, id: DisplayedItemRef) -> bool {
        if let Some(waves) = &self.waves {
            waves.selected_items.contains(&id)
        } else {
            false
        }
    }

    fn draw_var_values(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
        let Some(waves) = &self.waves else { return };
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        let rect = response.rect;
        let container_rect = Rect::from_min_size(Pos2::ZERO, rect.size());
        let to_screen = RectTransform::from_to(container_rect, rect);
        let cfg = DrawConfig::new(
            rect.height(),
            self.config.layout.waveforms_line_height,
            self.config.layout.waveforms_text_size,
        );
        let frame_width = rect.width();

        painter.rect_filled(
            rect,
            Rounding::ZERO,
            self.config.theme.secondary_ui_color.background,
        );
        let ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            // This 0.5 is very odd, but it fixes the lines we draw being smushed out across two
            // pixels, resulting in dimmer colors https://github.com/emilk/egui/issues/1322
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y) + Vec2::new(0.5, 0.5)),
            theme: &self.config.theme,
        };

        let gap = ui.spacing().item_spacing.y * 0.5;
        let y_zero = to_screen.transform_pos(Pos2::ZERO).y;
        let ucursor = waves.cursor.as_ref().and_then(num::BigInt::to_biguint);

        // Add default margin as it was removed when creating the frame
        let rect_with_margin = Rect {
            min: rect.min + ui.spacing().item_spacing,
            max: rect.max,
        };

        let builder = UiBuilder::new().max_rect(rect_with_margin);
        ui.allocate_new_ui(builder, |ui| {
            let text_style = TextStyle::Monospace;
            ui.style_mut().override_text_style = Some(text_style);
            for (vidx, drawing_info) in waves
                .drawing_infos
                .iter()
                .sorted_by_key(|o| o.top() as i32)
                .enumerate()
            {
                let vidx = vidx.into();
                let next_y = ui.cursor().top();
                // In order to align the text in this view with the variable tree,
                // we need to keep track of how far away from the expected offset we are,
                // and compensate for it
                if next_y < drawing_info.top() {
                    ui.add_space(drawing_info.top() - next_y);
                }

                let backgroundcolor = &self.get_background_color(waves, drawing_info, vidx);
                self.draw_background(
                    drawing_info,
                    y_zero,
                    &ctx,
                    gap,
                    frame_width,
                    backgroundcolor,
                );
                match drawing_info {
                    ItemDrawingInfo::Variable(drawing_info) => {
                        if ucursor.as_ref().is_none() {
                            ui.label("");
                            continue;
                        }

                        let v = self.get_variable_value(
                            waves,
                            &drawing_info.displayed_field_ref,
                            &ucursor,
                        );
                        if let Some(v) = v {
                            ui.label(
                                RichText::new(v)
                                    .color(*self.config.theme.get_best_text_color(backgroundcolor)),
                            )
                            .context_menu(|ui| {
                                self.item_context_menu(
                                    Some(&FieldRef::without_fields(
                                        drawing_info.field_ref.root.clone(),
                                    )),
                                    msgs,
                                    ui,
                                    vidx,
                                );
                            });
                        }
                    }

                    ItemDrawingInfo::Divider(_) => {
                        ui.label("");
                    }
                    ItemDrawingInfo::Marker(numbered_cursor) => {
                        if let Some(cursor) = &waves.cursor {
                            let delta = time_string(
                                &(waves.numbered_marker_time(numbered_cursor.idx) - cursor),
                                &waves.inner.metadata().timescale,
                                &self.wanted_timeunit,
                                &self.get_time_format(),
                            );

                            ui.label(
                                RichText::new(format!("Δ: {delta}",))
                                    .color(*self.config.theme.get_best_text_color(backgroundcolor)),
                            )
                            .context_menu(|ui| {
                                self.item_context_menu(None, msgs, ui, vidx);
                            });
                        } else {
                            ui.label("");
                        }
                    }
                    ItemDrawingInfo::TimeLine(_) => {
                        ui.label("");
                    }
                    ItemDrawingInfo::Stream(_) => {
                        ui.label("");
                    }
                }
            }
        });
    }

    pub fn get_variable_value(
        &self,
        waves: &WaveData,
        displayed_field_ref: &DisplayedFieldRef,
        ucursor: &Option<num::BigUint>,
    ) -> Option<String> {
        if let Some(ucursor) = ucursor {
            let Some(DisplayedItem::Variable(displayed_variable)) =
                waves.displayed_items.get(&displayed_field_ref.item)
            else {
                return None;
            };
            let variable = &displayed_variable.variable_ref;
            let translator = waves
                .variable_translator(&displayed_field_ref.without_field(), &self.sys.translators);
            let meta = waves.inner.as_waves().unwrap().variable_meta(variable);

            let translation_result = waves
                .inner
                .as_waves()
                .unwrap()
                .query_variable(variable, ucursor)
                .ok()
                .flatten()
                .and_then(|q| q.current)
                .map(|(_time, value)| meta.and_then(|meta| translator.translate(&meta, &value)));

            if let Some(Ok(s)) = translation_result {
                let fields = s.format_flat(
                    &displayed_variable.format,
                    &displayed_variable.field_formats,
                    &self.sys.translators,
                );

                let subfield = fields
                    .iter()
                    .find(|res| res.names == displayed_field_ref.field);

                if let Some(SubFieldFlatTranslationResult {
                    names: _,
                    value: Some(TranslatedValue { value: v, kind: _ }),
                }) = subfield
                {
                    Some(v.clone())
                } else {
                    Some("-".to_string())
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn draw_background(
        &self,
        drawing_info: &ItemDrawingInfo,
        y_zero: f32,
        ctx: &DrawingContext<'_>,
        gap: f32,
        frame_width: f32,
        background_color: &Color32,
    ) {
        // Draw background
        let min = (ctx.to_screen)(0.0, drawing_info.top() - y_zero - gap);
        let max = (ctx.to_screen)(frame_width, drawing_info.bottom() - y_zero + gap);
        ctx.painter
            .rect_filled(Rect { min, max }, Rounding::ZERO, *background_color);
    }

    pub fn get_background_color(
        &self,
        waves: &WaveData,
        drawing_info: &ItemDrawingInfo,
        vidx: DisplayedItemIndex,
    ) -> Color32 {
        *waves
            .displayed_items
            .get(&waves.displayed_items_order[drawing_info.item_list_idx()])
            .and_then(super::displayed_item::DisplayedItem::background_color)
            .and_then(|color| self.config.theme.get_color(&color))
            .unwrap_or_else(|| self.get_default_alternating_background_color(vidx))
    }

    fn get_default_alternating_background_color(&self, vidx: DisplayedItemIndex) -> &Color32 {
        // Set background color
        if self.config.theme.alt_frequency != 0
            && (vidx.0 / self.config.theme.alt_frequency) % 2 == 1
        {
            &self.config.theme.canvas_colors.alt_background
        } else {
            &Color32::TRANSPARENT
        }
    }
}

fn variable_tooltip_text(meta: &Option<VariableMeta>, variable: &VariableRef) -> String {
    format!(
        "{}\nNum bits: {}\nType: {}\nDirection: {}",
        variable.full_path_string(),
        meta.as_ref()
            .and_then(|meta| meta.num_bits)
            .map_or_else(|| "unknown".to_string(), |num_bits| format!("{num_bits}")),
        meta.as_ref()
            .and_then(|meta| meta.variable_type)
            .map_or_else(
                || "unknown".to_string(),
                |variable_type| format!("{variable_type}")
            ),
        meta.as_ref()
            .and_then(|meta| meta.direction)
            .map_or_else(|| "unknown".to_string(), |direction| format!("{direction}"))
    )
}

fn scope_tooltip_text(wave: &WaveData, scope: &ScopeRef) -> String {
    let other = wave.inner.as_waves().unwrap().get_scope_tooltip_data(scope);
    if other.is_empty() {
        format!("{scope}")
    } else {
        format!("{scope}\n{other}")
    }
}
