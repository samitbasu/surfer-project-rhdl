use color_eyre::eyre::Context;
#[cfg(not(target_arch = "wasm32"))]
use eframe::egui::ViewportCommand;
use eframe::egui::{
    self, ecolor::Color32, style::Margin, FontSelection, Frame, Layout, Painter, RichText,
    ScrollArea, Sense, Style, TextStyle, WidgetText,
};
use eframe::emath::{Align, Pos2, Rect, RectTransform, Vec2};
use eframe::epaint::{text::LayoutJob, Rounding, Stroke};
use fzcmd::expand_command;
use itertools::Itertools;
use log::{info, warn};

#[cfg(feature = "performance_plot")]
use crate::benchmark::NUM_PERF_SAMPLES;
use crate::command_prompt::get_parser;
use crate::config::SurferTheme;
use crate::displayed_item::{
    draw_rename_window, DisplayedFieldRef, DisplayedItem, DisplayedItemIndex, DisplayedItemRef,
};
use crate::help::{
    draw_about_window, draw_control_help_window, draw_license_window, draw_quickstart_help_window,
};
use crate::time::time_string;
use crate::translation::{SubFieldFlatTranslationResult, TranslatedValue};
use crate::util::uint_idx_to_alpha_idx;
use crate::wave_container::{FieldRef, ScopeRef, VariableRef};
use crate::wave_source::LoadOptions;
use crate::{
    command_prompt::show_command_prompt, config::HierarchyStyle, hierarchy,
    translation::VariableInfo, wave_data::WaveData, Message, MoveDir, State,
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

pub enum ItemDrawingInfo {
    Variable(VariableDrawingInfo),
    Divider(DividerDrawingInfo),
    Marker(MarkerDrawingInfo),
    TimeLine(TimeLineDrawingInfo),
}

impl ItemDrawingInfo {
    pub fn top(&self) -> f32 {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.top,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.top,
        }
    }
    pub fn bottom(&self) -> f32 {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.bottom,
        }
    }
    pub fn item_list_idx(&self) -> usize {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.item_list_idx.0,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.item_list_idx.0,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.item_list_idx.0,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.item_list_idx.0,
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
        if let Some(scale) = self.ui_zoom_factor {
            if ctx.zoom_factor() != scale {
                ctx.set_zoom_factor(scale)
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
                self.add_overview_panel(ctx, waves, &mut msgs)
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
                .show(ctx, |ui| match self.config.layout.hierarchy_style {
                    HierarchyStyle::Separate => hierarchy::separate(self, ui, &mut msgs),
                    HierarchyStyle::Tree => hierarchy::tree(self, ui, &mut msgs),
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
                    // Remove margin so that we can draw background
                    .frame(Frame {
                        inner_margin: Margin::ZERO,
                        outer_margin: Margin::ZERO,
                        ..Default::default()
                    })
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

    pub fn draw_all_scopes(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        draw_variables: bool,
        ui: &mut egui::Ui,
        filter: &str,
    ) {
        for scope in wave.inner.root_scopes() {
            self.draw_selectable_child_or_orphan_scope(
                msgs,
                wave,
                &scope,
                draw_variables,
                ui,
                filter,
            );
        }
        if draw_variables {
            let scope = ScopeRef::empty();
            self.draw_variable_list(msgs, wave, ui, &scope, filter);
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

        if self.show_tooltip() {
            response = response.on_hover_text(scope_tooltip_text(wave, scope));
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
        draw_variables: bool,
        ui: &mut egui::Ui,
        filter: &str,
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

        if child_scopes.is_empty() && !draw_variables {
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
                self.draw_root_scope_view(msgs, wave, scope, draw_variables, ui, filter);
                if draw_variables {
                    self.draw_variable_list(msgs, wave, ui, scope, filter);
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
        wave: &WaveData,
        ui: &mut egui::Ui,
        scope: &ScopeRef,
        filter: &str,
    ) {
        for variable in self.filtered_variables(wave, filter, scope) {
            let meta = wave.inner.variable_meta(&variable).ok();
            let index = meta
                .as_ref()
                .and_then(|meta| meta.index.clone())
                .map(|index| format!(" {index}"))
                .unwrap_or_default();

            let direction = if self.show_variable_direction() {
                meta.as_ref()
                    .and_then(|meta| meta.direction.clone())
                    .map(|direction| format!("{} ", direction.get_icon()))
                    .unwrap_or_default()
            } else {
                String::new()
            };

            let variable_name = format!("{}{}{}", direction, variable.name.clone(), index);
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    let mut response = ui.add(egui::SelectableLabel::new(false, variable_name));
                    if self.show_tooltip() {
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
                            let var = displayed_variable;
                            let info = &displayed_variable.info;
                            let style = Style::default();
                            let mut layout_job = LayoutJob::default();
                            self.add_alpha_id(
                                draw_alpha,
                                vidx,
                                &style,
                                &mut layout_job,
                                Align::LEFT,
                            );
                            displayed_item.add_to_layout_job(
                                &self.config.theme.foreground,
                                &style,
                                &mut layout_job,
                            );

                            self.add_alpha_id(
                                draw_alpha,
                                vidx,
                                &style,
                                &mut layout_job,
                                Align::RIGHT,
                            );

                            self.draw_variable(
                                msgs,
                                vidx,
                                WidgetText::LayoutJob(layout_job),
                                *displayed_item_id,
                                FieldRef::without_fields(var.variable_ref.clone()),
                                &mut item_offsets,
                                info,
                                ui,
                            )
                        }
                        DisplayedItem::Divider(_) => self.draw_plain_item(
                            msgs,
                            vidx,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                            draw_alpha,
                        ),
                        DisplayedItem::Marker(_) => self.draw_plain_item(
                            msgs,
                            vidx,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                            draw_alpha,
                        ),
                        DisplayedItem::Placeholder(_) => self.draw_plain_item(
                            msgs,
                            vidx,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                            draw_alpha,
                        ),
                        DisplayedItem::TimeLine(_) => self.draw_plain_item(
                            msgs,
                            vidx,
                            displayed_item,
                            &mut item_offsets,
                            ui,
                            draw_alpha,
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

        if item_response.drag_released()
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
        name: WidgetText,
        displayed_id: DisplayedItemRef,
        field: FieldRef,
        drawing_infos: &mut Vec<ItemDrawingInfo>,
        info: &VariableInfo,
        ui: &mut egui::Ui,
    ) -> Rect {
        let draw_label = |ui: &mut egui::Ui| {
            let mut variable_label = ui
                .selectable_label(self.item_is_selected(vidx), name)
                .context_menu(|ui| {
                    self.item_context_menu(Some(&field), msgs, ui, vidx);
                })
                .interact(Sense::drag());

            if self.show_tooltip() {
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

        let displayed_field_ref = DisplayedFieldRef {
            item: displayed_id,
            field: field.field.clone(),
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
                            displayed_id,
                            new_path,
                            drawing_infos,
                            info,
                            ui,
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
        displayed_item: &DisplayedItem,
        drawing_infos: &mut Vec<ItemDrawingInfo>,
        ui: &mut egui::Ui,
        draw_alpha: bool,
    ) -> Rect {
        let mut draw_label = |ui: &mut egui::Ui| {
            let style = Style::default();
            let mut layout_job = LayoutJob::default();
            self.add_alpha_id(draw_alpha, vidx, &style, &mut layout_job, Align::LEFT);

            let text_color = self.get_item_text_color(displayed_item);

            displayed_item.add_to_layout_job(text_color, &style, &mut layout_job);
            self.add_alpha_id(draw_alpha, vidx, &style, &mut layout_job, Align::RIGHT);
            let item_label = ui
                .selectable_label(
                    self.item_is_selected(vidx),
                    WidgetText::LayoutJob(layout_job),
                )
                .context_menu(|ui| {
                    self.item_context_menu(None, msgs, ui, vidx);
                })
                .interact(Sense::drag());
            if item_label.clicked() {
                msgs.push(Message::FocusItem(vidx))
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
                }))
            }
            DisplayedItem::Marker(cursor) => {
                drawing_infos.push(ItemDrawingInfo::Marker(MarkerDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                    idx: cursor.idx,
                }))
            }
            DisplayedItem::TimeLine(_) => {
                drawing_infos.push(ItemDrawingInfo::TimeLine(TimeLineDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }))
            }
            &DisplayedItem::Variable(_) => {}
            &DisplayedItem::Placeholder(_) => {}
        }
        label.rect
    }

    fn add_alpha_id(
        &self,
        draw_alpha: bool,
        vidx: DisplayedItemIndex,
        style: &Style,
        layout_job: &mut LayoutJob,
        alignment: Align,
    ) {
        if draw_alpha && alignment == self.get_name_alignment() {
            let alpha_id = uint_idx_to_alpha_idx(
                vidx,
                self.waves
                    .as_ref()
                    .map_or(0, |waves| waves.displayed_items.len()),
            );
            let text = RichText::new(alpha_id)
                .background_color(self.config.theme.accent_info.background)
                .monospace()
                .color(self.config.theme.accent_info.foreground);
            if alignment == Align::LEFT {
                text.append_to(layout_job, style, FontSelection::Default, Align::Center);
                RichText::new(" ").append_to(
                    layout_job,
                    style,
                    FontSelection::Default,
                    Align::Center,
                );
            } else {
                RichText::new(" ").append_to(
                    layout_job,
                    style,
                    FontSelection::Default,
                    Align::Center,
                );
                text.append_to(layout_job, style, FontSelection::Default, Align::Center);
            }
        }
    }

    fn item_is_selected(&self, vidx: DisplayedItemIndex) -> bool {
        if let Some(waves) = &self.waves {
            waves.focused_item == Some(vidx)
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
        let cfg = DrawConfig::new(rect.height());
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
        let ucursor = waves.cursor.as_ref().and_then(|u| u.to_biguint());

        // Add default margin as it was removed when creating the frame
        let rect_with_margin = Rect {
            min: rect.min + ui.spacing().item_spacing,
            max: rect.max,
        };
        ui.allocate_ui_at_rect(rect_with_margin, |ui| {
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
            let meta = waves.inner.variable_meta(variable);

            let translation_result = waves
                .inner
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
            .and_then(|variable| variable.background_color())
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

fn variable_tooltip_text(wave: &WaveData, variable: &VariableRef) -> String {
    let meta = wave.inner.variable_meta(variable).ok();
    format!(
        "{}\nNum bits: {}\nType: {}\nDirection: {}",
        variable.full_path_string(),
        meta.as_ref()
            .and_then(|meta| meta.num_bits)
            .map(|num_bits| format!("{num_bits}"))
            .unwrap_or_else(|| "unknown".to_string()),
        meta.as_ref()
            .and_then(|meta| meta.variable_type)
            .map(|variable_type| format!("{variable_type}"))
            .unwrap_or_else(|| "unknown".to_string()),
        meta.and_then(|meta| meta.direction)
            .map(|direction| format!("{direction}"))
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
