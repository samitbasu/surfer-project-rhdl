use std::cmp::Ordering;
use std::collections::HashMap;

use color_eyre::eyre::WrapErr;
use eframe::egui::{self, Sense};
use eframe::emath::{Align2, RectTransform};
use eframe::epaint::{Color32, FontId, PathShape, Pos2, Rect, RectShape, Rounding, Stroke, Vec2};
use log::{error, warn};
use num::bigint::{ToBigInt, ToBigUint};
use num::ToPrimitive;
use rayon::prelude::{IntoParallelRefIterator, ParallelBridge, ParallelIterator};

use crate::clock_highlighting::draw_clock_edge;
use crate::config::SurferTheme;
use crate::displayed_item::DisplayedSignal;
use crate::translation::{
    SignalInfo, SubFieldFlatTranslationResult, TranslatedValue, TranslatorList, ValueKind,
};
use crate::view::{DrawConfig, DrawingContext, ItemDrawingInfo};
use crate::wave_container::{FieldRef, QueryResult, SignalRef};
use crate::wave_data::WaveData;
use crate::{displayed_item::DisplayedItem, CachedDrawData, Message, State};

pub struct DrawnRegion {
    inner: Option<TranslatedValue>,
    /// True if a transition should be drawn even if there is no change in the value
    /// between the previous and next pixels. Only used by the bool drawing logic to
    /// draw draw a vertical line and prevent apparent aliasing
    force_anti_alias: bool,
}

/// List of values to draw for a signal. It is an ordered list of values that should
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

struct SignalDrawCommands {
    clock_edges: Vec<f32>,
    signal_ref: SignalRef,
    local_commands: HashMap<Vec<String>, DrawingCommands>,
    local_msgs: Vec<Message>,
}

fn signal_draw_commands(
    displayed_signal: &DisplayedSignal,
    timestamps: &[(f32, num::BigUint)],
    waves: &WaveData,
    translators: &TranslatorList,
    view_width: f64,
) -> Option<SignalDrawCommands> {
    let mut clock_edges = vec![];
    let mut local_msgs = vec![];

    let meta = match waves
        .inner
        .signal_meta(&displayed_signal.signal_ref)
        .context("failed to get signal meta")
    {
        Ok(meta) => meta,
        Err(e) => {
            warn!("{e:#?}");
            return None;
        }
    };

    let translator = waves.signal_translator(
        &FieldRef {
            root: displayed_signal.signal_ref.clone(),
            field: vec![],
        },
        translators,
    );
    // we need to get the signal info here to get the correct info for aliases
    let info = translator.signal_info(&meta).unwrap();

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

        let query_result = waves.inner.query_signal(&displayed_signal.signal_ref, time);
        next_change = match &query_result {
            Ok(QueryResult {
                next: Some(timestamp),
                ..
            }) => waves
                .viewport
                .from_time(&timestamp.to_bigint().unwrap(), view_width) as f32,
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
                error!("Signal query error {e:#?}");
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
                    sig_name = displayed_signal.signal_ref.full_path_string()
                );
                error!("{e:#}");
                local_msgs.push(Message::ResetSignalFormat(FieldRef {
                    root: displayed_signal.signal_ref.clone(),
                    field: vec![],
                }));
                return None;
            }
        };

        let fields = translation_result
            .flatten(
                FieldRef {
                    root: displayed_signal.signal_ref.clone(),
                    field: vec![],
                },
                &waves.signal_format,
                translators,
            )
            .as_fields();

        for SubFieldFlatTranslationResult { names, value } in fields {
            let prev = prev_values.get(&names);

            // If the value changed between this and the previous pixel, we want to
            // draw a transition even if the translated value didn't change.  We
            // only want to do this for root signals, because resolving when a
            // sub-field change is tricky without more information from the
            // translators
            let anti_alias = &change_time > prev_time && names.is_empty();
            let new_value = prev != Some(&value);

            // This is not the value we drew last time
            if new_value || is_last_timestep || anti_alias {
                *prev_values.entry(names.clone()).or_insert(value.clone()) = value.clone();

                if let SignalInfo::Clock = info.get_subinfo(&names) {
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
                        if let SignalInfo::Bool | SignalInfo::Clock = info.get_subinfo(&names) {
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
    Some(SignalDrawCommands {
        clock_edges,
        signal_ref: displayed_signal.signal_ref.clone(),
        local_commands,
        local_msgs,
    })
}

impl State {
    pub fn invalidate_draw_commands(&mut self) {
        *self.sys.draw_data.borrow_mut() = None;
    }

    pub fn generate_draw_commands(
        &self,
        cfg: &DrawConfig,
        frame_width: f32,
        msgs: &mut Vec<Message>,
    ) {
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
                    let time = waves.viewport.to_time_f64(x as f64, frame_width);
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
                .displayed_items
                .par_iter()
                .filter_map(|item| match item {
                    DisplayedItem::Signal(signal_ref) => Some(signal_ref),
                    _ => None,
                })
                // Iterate over the signals, generating draw commands for all the
                // subfields
                .filter_map(|displayed_signal| {
                    signal_draw_commands(
                        displayed_signal,
                        &timestamps,
                        waves,
                        translators,
                        frame_width as f64,
                    )
                })
                .collect::<Vec<_>>();

            for SignalDrawCommands {
                clock_edges: mut new_clock_edges,
                signal_ref,
                local_commands,
                mut local_msgs,
            } in commands
            {
                msgs.append(&mut local_msgs);
                for (path, val) in local_commands {
                    draw_commands.insert(
                        FieldRef {
                            root: signal_ref.clone(),
                            field: path.clone(),
                        },
                        val,
                    );
                }
                clock_edges.append(&mut new_clock_edges)
            }
            let ticks = self.get_ticks(
                &waves.viewport,
                &waves.inner.metadata().timescale,
                frame_width,
                cfg.text_size,
            );

            *self.sys.draw_data.borrow_mut() = Some(CachedDrawData {
                draw_commands,
                clock_edges,
                ticks,
            });
        }
        self.sys.timing.borrow_mut().end("Generate draw commands");
    }

    pub fn draw_signals(
        &self,
        msgs: &mut Vec<Message>,
        item_offsets: &Vec<ItemDrawingInfo>,
        ui: &mut egui::Ui,
    ) {
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::drag());

        let cfg = DrawConfig::new(response.rect.size().y);
        // the draw commands have been invalidated, recompute
        if self.sys.draw_data.borrow().is_none()
            || Some(response.rect) != *self.sys.last_canvas_rect.borrow()
        {
            self.generate_draw_commands(&cfg, response.rect.width(), msgs);
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
                })
            }

            if ui.input(|i| i.zoom_delta()) != 1. {
                let mouse_ptr_timestamp = Some(
                    waves
                        .viewport
                        .to_time_f64(mouse_ptr_pos.x as f64, frame_width),
                );

                msgs.push(Message::CanvasZoom {
                    mouse_ptr_timestamp,
                    delta: ui.input(|i| i.zoom_delta()),
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
                })
            }
        });

        response.dragged_by(egui::PointerButton::Primary).then(|| {
            let x = pointer_pos_canvas.unwrap().x;
            let timestamp = waves.viewport.to_time_bigint(x as f64, frame_width);
            msgs.push(Message::CursorSet(timestamp));
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

        let gap = self.get_item_gap(item_offsets, &ctx);
        for (idx, drawing_info) in item_offsets.iter().enumerate() {
            let default_background_color =
                self.get_default_alternating_background_color(idx + waves.scroll);
            let background_color = *waves
                .displayed_items
                .get(drawing_info.signal_list_idx())
                .and_then(|signal| signal.background_color())
                .and_then(|color| self.config.theme.colors.get(&color))
                .unwrap_or(&default_background_color);

            // We draw in absolute coords, but the signal offset in the y
            // direction is also in absolute coordinates, so we need to
            // compensate for that
            let y_offset = drawing_info.offset() - to_screen.transform_pos(Pos2::ZERO).y;
            let min = (ctx.to_screen)(0.0, y_offset - gap);
            let max = (ctx.to_screen)(frame_width, y_offset + ctx.cfg.line_height + gap);
            ctx.painter
                .rect_filled(Rect { min, max }, Rounding::ZERO, background_color);
        }

        self.sys.timing.borrow_mut().start("Wave drawing");
        if let Some(draw_data) = &*self.sys.draw_data.borrow() {
            let clock_edges = &draw_data.clock_edges;
            let draw_commands = &draw_data.draw_commands;
            let draw_clock_edges = match clock_edges.as_slice() {
                [] => false,
                [_single] => true,
                [first, second, ..] => second - first > 15.,
            };
            let ticks = &draw_data.ticks;
            if !ticks.is_empty() && self.show_ticks.unwrap_or(self.config.layout.show_ticks()) {
                let stroke = Stroke {
                    color: self.config.ticks.style.color,
                    width: self.config.ticks.style.width,
                };

                for (_, x) in ticks {
                    self.draw_tick_line(*x, &mut ctx, &stroke)
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

            for drawing_info in item_offsets {
                // We draw in absolute coords, but the signal offset in the y
                // direction is also in absolute coordinates, so we need to
                // compensate for that
                let y_offset = drawing_info.offset() - to_screen.transform_pos(Pos2::ZERO).y;

                let color = waves
                    .displayed_items
                    .get(drawing_info.signal_list_idx())
                    .and_then(|signal| signal.color())
                    .and_then(|color| self.config.theme.colors.get(&color));

                match drawing_info {
                    ItemDrawingInfo::Signal(drawing_info) => {
                        if let Some(commands) = draw_commands.get(&drawing_info.field_ref) {
                            for (old, new) in
                                commands.values.iter().zip(commands.values.iter().skip(1))
                            {
                                let color = *color.unwrap_or(&self.config.theme.signal_default);
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
                    ItemDrawingInfo::Cursor(_) => {}
                    ItemDrawingInfo::TimeLine(_) => {
                        self.draw_ticks(color, ticks, &ctx, y_offset, Align2::CENTER_TOP);
                    }
                }
            }
        }
        self.sys.timing.borrow_mut().end("Wave drawing");

        waves.draw_cursor(
            &self.config.theme,
            &mut ctx,
            response.rect.size(),
            &waves.viewport,
        );

        waves.draw_cursors(
            &self.config.theme,
            &mut ctx,
            response.rect.size(),
            &waves.viewport,
        );

        self.draw_cursor_boxes(waves, &mut ctx, item_offsets, response.rect.size(), gap);

        self.draw_mouse_gesture_widget(waves, pointer_pos_canvas, &response, msgs, &mut ctx);
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

            let (mut old_height, old_color, old_bg) =
                prev_result
                    .value
                    .bool_drawing_spec(color, &self.config.theme, prev_result.kind);
            let (mut new_height, _, _) =
                new_result
                    .value
                    .bool_drawing_spec(color, &self.config.theme, new_result.kind);

            let stroke = Stroke {
                color: old_color,
                width: self.config.theme.linewidth,
            };

            if force_anti_alias {
                old_height = 0.;
                new_height = 1.;
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
}

trait SignalExt {
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
            ValueKind::HighImp => theme.signal_highimp,
            ValueKind::Undef => theme.signal_undef,
            ValueKind::DontCare => theme.signal_dontcare,
            ValueKind::Warn => theme.signal_undef,
            ValueKind::Custom(custom_color) => *custom_color,
            ValueKind::Weak => theme.signal_weak,
            ValueKind::Normal => user_color,
        }
    }
}

impl SignalExt for String {
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
