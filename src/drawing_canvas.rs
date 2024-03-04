use std::cmp::Ordering;
use std::collections::HashMap;

use color_eyre::eyre::WrapErr;
use eframe::egui::{self, Response, Sense};
use eframe::emath::{Align2, RectTransform};
use eframe::epaint::{Color32, FontId, PathShape, Pos2, Rect, RectShape, Rounding, Stroke, Vec2};
use itertools::Itertools;
use log::{error, warn};
use num::bigint::{ToBigInt, ToBigUint};
use num::{BigInt, ToPrimitive};
use rayon::prelude::{IntoParallelRefIterator, ParallelBridge, ParallelIterator};

use crate::clock_highlighting::draw_clock_edge;
use crate::config::SurferTheme;
use crate::displayed_item::DisplayedVariable;
use crate::translation::{
    SubFieldFlatTranslationResult, TranslatedValue, TranslatorList, ValueKind, VariableInfo,
};
use crate::view::{DrawConfig, DrawingContext, ItemDrawingInfo};
use crate::viewport::Viewport;
use crate::wave_container::{FieldRef, QueryResult, VariableRef};
use crate::wave_data::WaveData;
use crate::{displayed_item::DisplayedItem, CachedDrawData, Message, State};

pub struct DrawnRegion {
    inner: Option<TranslatedValue>,
    /// True if a transition should be drawn even if there is no change in the value
    /// between the previous and next pixels. Only used by the bool drawing logic to
    /// draw draw a vertical line and prevent apparent aliasing
    force_anti_alias: bool,
}

/// List of values to draw for a variable. It is an ordered list of values that should
/// be drawn at the *start time* until the *start time* of the next value
pub struct DrawingCommands {
    is_bool: bool,
    values: Vec<(f32, DrawnRegion)>,
}

impl DrawingCommands {
    pub fn new_bool() -> Self {
        Self {
            values: vec![],
            is_bool: true,
        }
    }

    pub fn new_wide() -> Self {
        Self {
            values: vec![],
            is_bool: false,
        }
    }

    pub fn push(&mut self, val: (f32, DrawnRegion)) {
        self.values.push(val)
    }
}

struct VariableDrawCommands {
    clock_edges: Vec<f32>,
    variable_ref: VariableRef,
    local_commands: HashMap<Vec<String>, DrawingCommands>,
    local_msgs: Vec<Message>,
}

fn variable_draw_commands(
    displayed_variable: &DisplayedVariable,
    timestamps: &[(f32, num::BigUint)],
    waves: &WaveData,
    translators: &TranslatorList,
    view_width: f32,
    viewport_idx: usize,
) -> Option<VariableDrawCommands> {
    let mut clock_edges = vec![];
    let mut local_msgs = vec![];

    let meta = match waves
        .inner
        .variable_meta(&displayed_variable.variable_ref)
        .context("failed to get variable meta")
    {
        Ok(meta) => meta,
        Err(e) => {
            warn!("{e:#?}");
            return None;
        }
    };

    let translator = waves.variable_translator(
        &FieldRef::without_fields(displayed_variable.variable_ref.clone()),
        translators,
    );
    // we need to get the variable info here to get the correct info for aliases
    let info = translator.variable_info(&meta).unwrap();

    let mut local_commands: HashMap<Vec<_>, _> = HashMap::new();

    let mut prev_values = HashMap::new();

    // In order to insert a final draw command at the end of a trace,
    // we need to know if this is the last timestamp to draw
    let end_pixel = timestamps.iter().last().map(|t| t.0).unwrap_or_default();
    // The first pixel we actually draw is the second pixel in the
    // list, since we skip one pixel to have a previous value
    let start_pixel = timestamps.get(1).map(|t| t.0).unwrap_or_default();

    // Iterate over all the time stamps to draw on
    let mut next_change = timestamps.first().map(|t| t.0).unwrap_or_default();
    for ((_, prev_time), (pixel, time)) in timestamps.iter().zip(timestamps.iter().skip(1)) {
        let is_last_timestep = pixel == &end_pixel;
        let is_first_timestep = pixel == &start_pixel;

        if *pixel < next_change && !is_first_timestep && !is_last_timestep {
            continue;
        }

        let query_result = waves
            .inner
            .query_variable(&displayed_variable.variable_ref, time);
        next_change = match &query_result {
            Ok(QueryResult {
                next: Some(timestamp),
                ..
            }) => waves.viewports[viewport_idx].pixel_from_time(
                &timestamp.to_bigint().unwrap(),
                view_width,
                &waves.num_timestamps,
            ),
            // If we don't have a next timestamp, we don't need to recheck until the last time
            // step
            Ok(_) => timestamps.last().map(|t| t.0).unwrap_or_default(),
            // If we get an error here, we'll let the next match block handle it, but we'll take
            // note that we need to recheck every pixel until the end
            _ => timestamps.first().map(|t| t.0).unwrap_or_default(),
        };

        let (change_time, val) = match query_result {
            Ok(QueryResult {
                current: Some((change_time, val)),
                ..
            }) => (change_time, val),
            Ok(QueryResult { current: None, .. }) => continue,
            Err(e) => {
                error!("Variable query error {e:#?}");
                continue;
            }
        };

        // Check if the value remains unchanged between this pixel
        // and the last
        if &change_time < prev_time && !is_first_timestep && !is_last_timestep {
            continue;
        }

        let translation_result = match translator.translate(&meta, &val) {
            Ok(result) => result,
            Err(e) => {
                error!(
                    "{translator_name} for {sig_name} failed. Disabling:",
                    translator_name = translator.name(),
                    sig_name = displayed_variable.variable_ref.full_path_string()
                );
                error!("{e:#}");
                local_msgs.push(Message::ResetVariableFormat(FieldRef::without_fields(
                    displayed_variable.variable_ref.clone(),
                )));
                return None;
            }
        };

        let fields = translation_result
            .flatten(
                FieldRef::without_fields(displayed_variable.variable_ref.clone()),
                &waves.variable_format,
                translators,
            )
            .as_fields();

        for SubFieldFlatTranslationResult { names, value } in fields {
            let prev = prev_values.get(&names);

            // If the value changed between this and the previous pixel, we want to
            // draw a transition even if the translated value didn't change.  We
            // only want to do this for root variables, because resolving when a
            // sub-field change is tricky without more information from the
            // translators
            let anti_alias =
                &change_time > prev_time && names.is_empty() && waves.inner.wants_anti_aliasing();
            let new_value = prev != Some(&value);

            // This is not the value we drew last time
            if new_value || is_last_timestep || anti_alias {
                *prev_values.entry(names.clone()).or_insert(value.clone()) = value.clone();

                if let VariableInfo::Clock = info.get_subinfo(&names) {
                    match value.as_ref().map(|result| result.value.as_str()) {
                        Some("1") => {
                            if !is_last_timestep && !is_first_timestep {
                                clock_edges.push(*pixel)
                            }
                        }
                        Some(_) => {}
                        None => {}
                    }
                }

                local_commands
                    .entry(names.clone())
                    .or_insert_with(|| {
                        if let VariableInfo::Bool | VariableInfo::Clock = info.get_subinfo(&names) {
                            DrawingCommands::new_bool()
                        } else {
                            DrawingCommands::new_wide()
                        }
                    })
                    .push((
                        *pixel,
                        DrawnRegion {
                            inner: value,
                            force_anti_alias: anti_alias && !new_value,
                        },
                    ))
            }
        }
    }
    Some(VariableDrawCommands {
        clock_edges,
        variable_ref: displayed_variable.variable_ref.clone(),
        local_commands,
        local_msgs,
    })
}

impl State {
    pub fn invalidate_draw_commands(&mut self) {
        if let Some(waves) = &self.waves {
            for viewport in 0..waves.viewports.len() {
                self.sys.draw_data.borrow_mut()[viewport] = None;
            }
        }
    }

    pub fn generate_draw_commands(
        &self,
        cfg: &DrawConfig,
        frame_width: f32,
        msgs: &mut Vec<Message>,
        viewport_idx: usize,
    ) {
        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().start("Generate draw commands");
        let mut draw_commands = HashMap::new();
        if let Some(waves) = &self.waves {
            let max_time = waves.num_timestamps.clone().to_f64().unwrap_or(f64::MAX);
            let mut clock_edges = vec![];
            // Compute which timestamp to draw in each pixel. We'll draw from -transition_width to
            // width + transition_width in order to draw initial transitions outside the screen
            let mut timestamps = (-cfg.max_transition_width
                ..(frame_width as i32 + cfg.max_transition_width))
                .par_bridge()
                .filter_map(|x| {
                    let time = waves.viewports[viewport_idx]
                        .to_time_f64(x as f64, frame_width, &waves.num_timestamps)
                        .0;
                    if time < 0. || time > max_time {
                        None
                    } else {
                        Some((x as f32, time.to_biguint().unwrap_or_default()))
                    }
                })
                .collect::<Vec<_>>();
            timestamps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

            let translators = &self.sys.translators;
            let commands = waves
                .displayed_items_order
                .par_iter()
                .map(|id| waves.displayed_items.get(id))
                .filter_map(|item| match item {
                    Some(DisplayedItem::Variable(variable_ref)) => Some(variable_ref),
                    _ => None,
                })
                // Iterate over the variables, generating draw commands for all the
                // subfields
                .filter_map(|displayed_variable| {
                    variable_draw_commands(
                        displayed_variable,
                        &timestamps,
                        waves,
                        translators,
                        frame_width,
                        viewport_idx,
                    )
                })
                .collect::<Vec<_>>();

            for VariableDrawCommands {
                clock_edges: mut new_clock_edges,
                variable_ref,
                local_commands,
                mut local_msgs,
            } in commands
            {
                msgs.append(&mut local_msgs);
                for (path, val) in local_commands {
                    draw_commands.insert(
                        FieldRef {
                            root: variable_ref.clone(),
                            field: path.clone(),
                        },
                        val,
                    );
                }
                clock_edges.append(&mut new_clock_edges)
            }
            let ticks = waves.get_ticks(
                &waves.viewports[viewport_idx],
                &waves.inner.metadata().timescale,
                frame_width,
                cfg.text_size,
                &self.wanted_timeunit,
                &self.get_time_format(),
                &self.config,
            );

            self.sys.draw_data.borrow_mut()[viewport_idx] = Some(CachedDrawData {
                draw_commands,
                clock_edges,
                ticks,
            });
        }
        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().end("Generate draw commands");
    }

    pub fn draw_items(&mut self, msgs: &mut Vec<Message>, ui: &mut egui::Ui, viewport_idx: usize) {
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::drag());

        let cfg = DrawConfig::new(response.rect.size().y);
        // the draw commands have been invalidated, recompute
        if self.sys.draw_data.borrow()[viewport_idx].is_none()
            || Some(response.rect) != *self.sys.last_canvas_rect.borrow()
        {
            self.generate_draw_commands(&cfg, response.rect.width(), msgs, viewport_idx);
            *self.sys.last_canvas_rect.borrow_mut() = Some(response.rect);
        }

        let Some(waves) = &self.waves else { return };
        let container_rect = Rect::from_min_size(Pos2::ZERO, response.rect.size());
        let to_screen = RectTransform::from_to(container_rect, response.rect);
        let frame_width = response.rect.width();
        let pointer_pos_global = ui.input(|i| i.pointer.interact_pos());
        let pointer_pos_canvas = pointer_pos_global.map(|p| to_screen.inverse().transform_pos(p));

        if ui.ui_contains_pointer() {
            let pointer_pos = pointer_pos_global.unwrap();
            let scroll_delta = ui.input(|i| i.scroll_delta);
            let mouse_ptr_pos = to_screen.inverse().transform_pos(pointer_pos);
            if scroll_delta != Vec2::ZERO {
                msgs.push(Message::CanvasScroll {
                    delta: ui.input(|i| i.scroll_delta),
                    viewport_idx,
                })
            }

            if ui.input(|i| i.zoom_delta()) != 1. {
                let mouse_ptr = Some(waves.viewports[viewport_idx].to_time_bigint(
                    mouse_ptr_pos.x,
                    frame_width,
                    &waves.num_timestamps,
                ));

                msgs.push(Message::CanvasZoom {
                    mouse_ptr,
                    delta: ui.input(|i| i.zoom_delta()),
                    viewport_idx,
                })
            }
        }

        ui.input(|i| {
            // If we have a single touch, we'll interpret that as a pan
            if i.any_touches() && i.multi_touch().is_none() {
                msgs.push(Message::CanvasScroll {
                    delta: Vec2 {
                        x: i.pointer.delta().y,
                        y: i.pointer.delta().x,
                    },
                    viewport_idx: 0,
                })
            }
        });

        response.dragged_by(egui::PointerButton::Primary).then(|| {
            if let Some(snap_point) =
                self.snap_to_edge(pointer_pos_canvas, waves, frame_width, viewport_idx)
            {
                msgs.push(Message::CursorSet(snap_point))
            }
        });

        painter.rect_filled(
            response.rect,
            Rounding::ZERO,
            self.config.theme.canvas_colors.background,
        );

        response
            .drag_started_by(egui::PointerButton::Middle)
            .then(|| msgs.push(Message::SetDragStart(pointer_pos_canvas)));

        let mut ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            // This 0.5 is very odd, but it fixes the lines we draw being smushed out across two
            // pixels, resulting in dimmer colors https://github.com/emilk/egui/issues/1322
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y) + Vec2::new(0.5, 0.5)),
            theme: &self.config.theme,
        };

        let gap = ui.spacing().item_spacing.y * 0.5;
        // We draw in absolute coords, but the variable offset in the y
        // direction is also in absolute coordinates, so we need to
        // compensate for that
        let y_zero = to_screen.transform_pos(Pos2::ZERO).y;
        for (idx, drawing_info) in waves.drawing_infos.iter().enumerate() {
            self.draw_background(idx, waves, drawing_info, y_zero, &ctx, gap, frame_width);
        }

        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().start("Wave drawing");
        if let Some(draw_data) = &self.sys.draw_data.borrow()[viewport_idx] {
            let clock_edges = &draw_data.clock_edges;
            let draw_commands = &draw_data.draw_commands;
            let draw_clock_edges = match clock_edges.as_slice() {
                [] => false,
                [_single] => true,
                [first, second, ..] => second - first > 15.,
            };
            let ticks = &draw_data.ticks;
            if !ticks.is_empty() && self.show_ticks() {
                let stroke = Stroke {
                    color: self.config.ticks.style.color,
                    width: self.config.ticks.style.width,
                };

                for (_, x) in ticks {
                    waves.draw_tick_line(*x, &mut ctx, &stroke)
                }
            }

            if draw_clock_edges {
                let mut last_edge = 0.0;
                let mut cycle = false;
                for current_edge in clock_edges {
                    draw_clock_edge(last_edge, *current_edge, cycle, &mut ctx, &self.config);
                    cycle = !cycle;
                    last_edge = *current_edge;
                }
            }
            let zero_y = to_screen.transform_pos(Pos2::ZERO).y;
            for drawing_info in &waves.drawing_infos {
                // We draw in absolute coords, but the variable offset in the y
                // direction is also in absolute coordinates, so we need to
                // compensate for that
                let y_offset = drawing_info.top() - zero_y;

                let color = waves
                    .displayed_items_order
                    .get(drawing_info.item_list_idx())
                    .and_then(|id| waves.displayed_items.get(id))
                    .and_then(|variable| variable.color())
                    .and_then(|color| self.config.theme.colors.get(&color));

                match drawing_info {
                    ItemDrawingInfo::Variable(drawing_info) => {
                        if let Some(commands) = draw_commands.get(&drawing_info.field_ref) {
                            for (old, new) in
                                commands.values.iter().zip(commands.values.iter().skip(1))
                            {
                                let color = *color.unwrap_or(&self.config.theme.variable_default);
                                if commands.is_bool {
                                    self.draw_bool_transition(
                                        (old, new),
                                        new.1.force_anti_alias,
                                        color,
                                        y_offset,
                                        &mut ctx,
                                    )
                                } else {
                                    self.draw_region((old, new), color, y_offset, &mut ctx)
                                }
                            }
                        }
                    }
                    ItemDrawingInfo::Divider(_) => {}
                    ItemDrawingInfo::Marker(_) => {}
                    ItemDrawingInfo::TimeLine(_) => {
                        waves.draw_ticks(
                            color,
                            ticks,
                            &ctx,
                            y_offset,
                            Align2::CENTER_TOP,
                            &self.config,
                        );
                    }
                }
            }
        }
        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().end("Wave drawing");

        waves.draw_cursor(
            &self.config.theme,
            &mut ctx,
            response.rect.size(),
            &waves.viewports[viewport_idx],
        );

        waves.draw_markers(
            &self.config.theme,
            &mut ctx,
            response.rect.size(),
            &waves.viewports[viewport_idx],
        );

        self.draw_marker_boxes(
            waves,
            &mut ctx,
            response.rect.size().x,
            gap,
            &waves.viewports[viewport_idx],
            y_zero,
        );

        self.draw_mouse_gesture_widget(
            waves,
            pointer_pos_canvas,
            &response,
            msgs,
            &mut ctx,
            viewport_idx,
        );
        self.handle_canvas_context_menu(response, waves, to_screen, &mut ctx, msgs, viewport_idx);
    }

    fn draw_region(
        &self,
        ((old_x, prev_region), (new_x, _)): (&(f32, DrawnRegion), &(f32, DrawnRegion)),
        user_color: Color32,
        offset: f32,
        ctx: &mut DrawingContext,
    ) {
        if let Some(prev_result) = &prev_region.inner {
            let stroke = Stroke {
                color: prev_result.kind.color(user_color, ctx.theme),
                width: self.config.theme.linewidth,
            };

            let transition_width = (new_x - old_x).min(6.);

            let trace_coords = |x, y| (ctx.to_screen)(x, y * ctx.cfg.line_height + offset);

            ctx.painter.add(PathShape::line(
                vec![
                    trace_coords(*old_x, 0.5),
                    trace_coords(old_x + transition_width / 2., 1.0),
                    trace_coords(new_x - transition_width / 2., 1.0),
                    trace_coords(*new_x, 0.5),
                    trace_coords(new_x - transition_width / 2., 0.0),
                    trace_coords(old_x + transition_width / 2., 0.0),
                    trace_coords(*old_x, 0.5),
                ],
                stroke,
            ));

            let text_size = ctx.cfg.text_size;
            let char_width = text_size * (20. / 31.);

            let text_area = (new_x - old_x) - transition_width;
            let num_chars = (text_area / char_width).floor();
            let fits_text = num_chars >= 1.;

            if fits_text {
                let content = if prev_result.value.len() > num_chars as usize {
                    prev_result
                        .value
                        .chars()
                        .take(num_chars as usize - 1)
                        .chain(['…'])
                        .collect::<String>()
                } else {
                    prev_result.value.to_string()
                };

                ctx.painter.text(
                    trace_coords(*old_x + transition_width, 0.5),
                    Align2::LEFT_CENTER,
                    content,
                    FontId::monospace(text_size),
                    self.config.theme.foreground,
                );
            }
        }
    }

    fn draw_bool_transition(
        &self,
        ((old_x, prev_region), (new_x, new_region)): (&(f32, DrawnRegion), &(f32, DrawnRegion)),
        force_anti_alias: bool,
        color: Color32,
        offset: f32,
        ctx: &mut DrawingContext,
    ) {
        if let (Some(prev_result), Some(new_result)) = (&prev_region.inner, &new_region.inner) {
            let trace_coords = |x, y| (ctx.to_screen)(x, y * ctx.cfg.line_height + offset);

            let (old_height, old_color, old_bg) =
                prev_result
                    .value
                    .bool_drawing_spec(color, &self.config.theme, prev_result.kind);
            let (new_height, _, _) =
                new_result
                    .value
                    .bool_drawing_spec(color, &self.config.theme, new_result.kind);

            let stroke = Stroke {
                color: old_color,
                width: self.config.theme.linewidth,
            };

            if force_anti_alias {
                ctx.painter.add(PathShape::line(
                    vec![trace_coords(*new_x, 0.0), trace_coords(*new_x, 1.0)],
                    stroke,
                ));
            }

            ctx.painter.add(PathShape::line(
                vec![
                    trace_coords(*old_x, 1. - old_height),
                    trace_coords(*new_x, 1. - old_height),
                    trace_coords(*new_x, 1. - new_height),
                ],
                stroke,
            ));

            if let Some(old_bg) = old_bg {
                ctx.painter.add(RectShape {
                    fill: old_bg,
                    rect: Rect {
                        min: (ctx.to_screen)(*old_x, offset),
                        max: (ctx.to_screen)(*new_x, offset + ctx.cfg.line_height),
                    },
                    rounding: Rounding::ZERO,
                    stroke: Stroke {
                        width: 0.,
                        ..Default::default()
                    },
                    fill_texture_id: Default::default(),
                    uv: Rect::ZERO,
                });
            }
        }
    }

    fn handle_canvas_context_menu(
        &self,
        response: Response,
        waves: &WaveData,
        to_screen: RectTransform,
        ctx: &mut DrawingContext,
        msgs: &mut Vec<Message>,
        viewport_idx: usize,
    ) {
        let size = response.rect.size().clone();
        response.context_menu(|ui| {
            let offset = ui.spacing().menu_margin.left;
            let top_left = to_screen.inverse().transform_rect(ui.min_rect()).left_top()
                - Pos2 {
                    x: offset,
                    y: offset,
                };

            let snap_pos = self.snap_to_edge(Some(top_left.to_pos2()), waves, size.x, viewport_idx);

            if let Some(time) = snap_pos {
                self.draw_line(&time, ctx, size, &waves.viewports[viewport_idx], waves);
                ui.menu_button("Set marker", |ui| {
                    macro_rules! close_menu {
                        () => {{
                            ui.close_menu();
                            msgs.push(Message::RightCursorSet(None))
                        }};
                    }

                    for id in waves.markers.keys().sorted() {
                        ui.button(format!("{id}")).clicked().then(|| {
                            msgs.push(Message::SetMarker {
                                id: *id,
                                time: time.clone(),
                            });
                            close_menu!()
                        });
                    }
                    // At the moment we only support 255 markers, and the cursor is the 255th
                    if waves.markers.len() < 254 {
                        ui.button("New").clicked().then(|| {
                            // NOTE: Safe unwrap, we have at least one empty slot
                            let id = (0..254).find(|id| !waves.markers.contains_key(id)).unwrap();
                            msgs.push(Message::SetMarker { id, time });
                            close_menu!()
                        });
                    }
                });
            }
        });
    }

    /// Takes a pointer pos in the canvas and returns a position that is snapped to transitions
    /// if the cursor is close enough to any transition. If the cursor is on the canvas
    /// a point will be returned, otherwise `None`
    fn snap_to_edge(
        &self,
        pointer_pos_canvas: Option<Pos2>,
        waves: &WaveData,
        frame_width: f32,
        viewport_idx: usize,
    ) -> Option<BigInt> {
        let Some(pos) = pointer_pos_canvas else {
            return None;
        };
        let viewport = &waves.viewports[viewport_idx];
        let timestamp = viewport.to_time_bigint(pos.x, frame_width, &waves.num_timestamps);
        if let Some(utimestamp) = timestamp.to_biguint() {
            if let Some(vidx) = waves.get_item_at_y(pos.y) {
                if let Some(id) = waves.displayed_items_order.get(vidx) {
                    if let DisplayedItem::Variable(variable) = &waves.displayed_items[id] {
                        if let Ok(res) = waves
                            .inner
                            .query_variable(&variable.variable_ref, &utimestamp)
                        {
                            let prev_time = &res.current.unwrap().0.to_bigint().unwrap();
                            let next_time = &res.next.unwrap_or_default().to_bigint().unwrap();
                            let prev = viewport.pixel_from_time(
                                prev_time,
                                frame_width,
                                &waves.num_timestamps,
                            );
                            let next = viewport.pixel_from_time(
                                next_time,
                                frame_width,
                                &waves.num_timestamps,
                            );
                            if (prev - pos.x).abs() < (next - pos.x).abs() {
                                if (prev - pos.x).abs() <= self.config.snap_distance {
                                    return Some(prev_time.clone());
                                }
                            } else {
                                if (next - pos.x).abs() <= self.config.snap_distance {
                                    return Some(next_time.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        Some(timestamp)
    }

    pub fn draw_line(
        &self,
        time: &BigInt,
        ctx: &mut DrawingContext,
        size: Vec2,
        viewport: &Viewport,
        waves: &WaveData,
    ) {
        let x = viewport.pixel_from_time(time, size.x, &waves.num_timestamps);

        let stroke = Stroke {
            color: self.config.theme.cursor.color,
            width: self.config.theme.cursor.width,
        };
        ctx.painter.line_segment(
            [
                (ctx.to_screen)(x + 0.5, -0.5),
                (ctx.to_screen)(x + 0.5, size.y),
            ],
            stroke,
        )
    }
}

impl WaveData {}

trait VariableExt {
    fn bool_drawing_spec(
        &self,
        user_color: Color32,
        theme: &SurferTheme,
        value_kind: ValueKind,
    ) -> (f32, Color32, Option<Color32>);
}

impl ValueKind {
    fn color(&self, user_color: Color32, theme: &SurferTheme) -> Color32 {
        match self {
            ValueKind::HighImp => theme.variable_highimp,
            ValueKind::Undef => theme.variable_undef,
            ValueKind::DontCare => theme.variable_dontcare,
            ValueKind::Warn => theme.variable_undef,
            ValueKind::Custom(custom_color) => *custom_color,
            ValueKind::Weak => theme.variable_weak,
            ValueKind::Normal => user_color,
        }
    }
}

impl VariableExt for String {
    /// Return the height and color with which to draw this value if it is a boolean
    fn bool_drawing_spec(
        &self,
        user_color: Color32,
        theme: &SurferTheme,
        value_kind: ValueKind,
    ) -> (f32, Color32, Option<Color32>) {
        let color = value_kind.color(user_color, theme);
        let (height, background) = match (value_kind, self) {
            (ValueKind::HighImp, _) => (0.5, None),
            (ValueKind::Undef, _) => (0.5, None),
            (ValueKind::DontCare, _) => (0.5, None),
            (ValueKind::Warn, _) => (0.5, None),
            (ValueKind::Custom(_), _) => (0.5, None),
            (ValueKind::Weak, other) => {
                if other.to_lowercase() == "l" {
                    (0., None)
                } else {
                    (1., Some(color.gamma_multiply(0.2)))
                }
            }
            (ValueKind::Normal, other) => {
                if other == "0" {
                    (0., None)
                } else {
                    (1., Some(color.gamma_multiply(0.2)))
                }
            }
        };
        (height, color, background)
    }
}
