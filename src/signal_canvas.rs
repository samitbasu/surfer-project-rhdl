use std::collections::HashMap;

use eframe::egui::{self, Painter, Sense};
use eframe::emath::{self, Align2, RectTransform};
use eframe::epaint::{Color32, FontId, PathShape, Pos2, Rect, RectShape, Rounding, Stroke, Vec2};
use log::error;
use num::BigRational;
use num::ToPrimitive;

use crate::benchmark::{TimedRegion, TranslationTimings};
use crate::config::SurferTheme;
use crate::translation::{SignalInfo, ValueKind};
use crate::view::{time_string, ItemDrawingInfo};
use crate::{DisplayedItem, Message, State, VcdData};

#[derive(Clone, PartialEq, Copy)]
pub enum GestureKind {
    ZoomToFit,
    ZoomIn,
    ZoomOut,
    ScrollToEnd,
    ScrollToStart,
}

pub struct DrawnRegion {
    inner: Option<(String, ValueKind)>,
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

impl State {
    pub fn invalidate_draw_commands(&mut self) {
        *self.draw_commands.borrow_mut() = None;
    }

    pub fn generate_draw_commands(&self, cfg: &DrawConfig, width: f32, msgs: &mut Vec<Message>) {
        let mut draw_commands = HashMap::new();
        if let Some(vcd) = &self.vcd {
            let frame_width = width;
            let max_time = BigRational::from_integer(vcd.num_timestamps.clone());
            let mut timings = TranslationTimings::new();
            let mut clock_edges = vec![];
            // Compute which timestamp to draw in each pixel. We'll draw from -transition_width to
            // width + transition_width in order to draw initial transitions outside the screen
            let timestamps = (-cfg.max_transition_width
                ..(frame_width as i32 + cfg.max_transition_width))
                .filter_map(|x| {
                    let time = vcd.viewport.to_time(x as f64, frame_width);
                    if time < BigRational::from_float(0.).unwrap() || time > max_time {
                        None
                    } else {
                        Some((x as f32, time.to_integer().to_biguint().unwrap()))
                    }
                })
                .collect::<Vec<_>>();

            vcd.signals
                .iter()
                .filter_map(|item| match item {
                    DisplayedItem::Signal(idx) => Some(idx),
                    _ => None,
                })
                .map(|displayed_signal| {
                    let idx = displayed_signal.idx;
                    // check if the signal is an alias
                    // if so get the real signal
                    let signal = vcd.inner.signal_from_signal_idx(idx);
                    let real_idx = signal.real_idx();
                    if real_idx == idx {
                        (idx, signal)
                    } else {
                        (real_idx, vcd.inner.signal_from_signal_idx(real_idx))
                    }
                })
                // Iterate over the signals, generating draw commands for all the
                // subfields
                .for_each(|(idx, sig)| {
                    let translator = vcd.signal_translator((idx, vec![]), &self.translators);
                    // we need to get the signal info here to get the correct info for aliases
                    let info = translator.signal_info(&sig, &vcd.signal_name(idx)).unwrap();

                    let mut local_commands: HashMap<Vec<_>, _> = HashMap::new();

                    let mut prev_values = HashMap::new();

                    // In order to insert a final draw command at the end of a trace,
                    // we need to know if this is the last timestamp to draw
                    let end_pixel = timestamps.iter().last().map(|t| t.0).unwrap_or_default();
                    // The first pixel we actually draw is the second pixel in the
                    // list, since we skip one pixel to have a previous value
                    let start_pixel = timestamps
                        .iter()
                        .skip(1)
                        .next()
                        .map(|t| t.0)
                        .unwrap_or_default();

                    // Iterate over all the time stamps to draw on
                    for ((_, prev_time), (pixel, time)) in
                        timestamps.iter().zip(timestamps.iter().skip(1))
                    {
                        let (change_time, val) =
                            if let Ok(v) = sig.query_val_on_tmln(&time, &vcd.inner) {
                                v
                            } else {
                                // If there is no value here, skip this iteration
                                continue;
                            };

                        let is_last_timestep = pixel == &end_pixel;
                        let is_first_timestep = pixel == &start_pixel;

                        // Check if the value remains unchanged between this pixel
                        // and the last
                        if &change_time < prev_time && !is_first_timestep && !is_last_timestep {
                            continue;
                        }

                        // Perform the translation
                        let mut duration = TimedRegion::started();

                        let translation_result = match translator.translate(&sig, &val) {
                            Ok(result) => result,
                            Err(e) => {
                                error!(
                                    "{translator_name} for {sig_name} failed. Disabling:",
                                    translator_name = translator.name(),
                                    sig_name = sig.name()
                                );
                                error!("{e:#}");
                                msgs.push(Message::ResetSignalFormat((idx, vec![])));
                                return;
                            }
                        };

                        duration.stop();
                        timings.push_timing(&translator.name(), None, duration.secs());
                        let fields = translation_result
                            .flatten((idx, vec![]), &vcd.signal_format, &self.translators)
                            .as_fields();

                        for (path, value) in fields {
                            let prev = prev_values.get(&path);

                            // This is not the value we drew last time
                            if prev != Some(&value) || is_last_timestep {
                                *prev_values.entry(path.clone()).or_insert(value.clone()) =
                                    value.clone();

                                if let SignalInfo::Clock = info.get_subinfo(&path) {
                                    match value.as_ref().map(|(val, _)| val.as_str()) {
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
                                    .entry(path.clone())
                                    .or_insert_with(|| {
                                        if let SignalInfo::Bool | SignalInfo::Clock =
                                            info.get_subinfo(&path)
                                        {
                                            DrawingCommands::new_bool()
                                        } else {
                                            DrawingCommands::new_wide()
                                        }
                                    })
                                    .push((*pixel, DrawnRegion { inner: value }))
                            }
                        }
                    }
                    // Append the signal index to the fields
                    local_commands.into_iter().for_each(|(path, val)| {
                        draw_commands.insert((sig.real_idx().clone(), path), val);
                    });
                });

            *self.draw_commands.borrow_mut() = Some(draw_commands);
        }
    }

    pub fn draw_signals(
        &self,
        msgs: &mut Vec<Message>,
        signal_offsets: &Vec<ItemDrawingInfo>,
        ui: &mut egui::Ui,
    ) {
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::drag());

        let cfg = DrawConfig {
            canvas_height: response.rect.size().y,
            line_height: 16.,
            max_transition_width: 6,
        };
        // the draw commands have been invalidated, recompute
        if self.draw_commands.borrow().is_none() {
            self.generate_draw_commands(&cfg, response.rect.width(), msgs);
        }

        let Some(vcd) = &self.vcd else { return };
        let container_rect = Rect::from_min_size(Pos2::ZERO, response.rect.size());
        let to_screen = emath::RectTransform::from_to(container_rect, response.rect);
        let frame_width = response.rect.width();
        let pointer_pos_global = ui.input(|i| i.pointer.interact_pos());
        let pointer_pos_canvas = pointer_pos_global.map(|p| to_screen.inverse().transform_pos(p));
        let pointer_in_canvas = pointer_pos_global
            .map(|p| to_screen.transform_rect(container_rect).contains(p))
            .unwrap_or(false);

        if pointer_in_canvas {
            let pointer_pos = pointer_pos_global.unwrap();
            let scroll_delta = ui.input(|i| i.scroll_delta);
            let mouse_ptr_pos = to_screen.inverse().transform_pos(pointer_pos);
            if scroll_delta != Vec2::ZERO {
                msgs.push(Message::CanvasScroll {
                    delta: ui.input(|i| i.scroll_delta),
                })
            }

            if ui.input(|i| i.zoom_delta()) != 1. {
                let mouse_ptr_timestamp = vcd
                    .viewport
                    .to_time(mouse_ptr_pos.x as f64, frame_width)
                    .to_f64();

                msgs.push(Message::CanvasZoom {
                    mouse_ptr_timestamp,
                    delta: ui.input(|i| i.zoom_delta()),
                })
            }
        }

        response.dragged_by(egui::PointerButton::Primary).then(|| {
            let x = pointer_pos_canvas.unwrap().x;
            let timestamp = vcd.viewport.to_time(x as f64, frame_width);
            msgs.push(Message::CursorSet(timestamp.round().to_integer()));
        });

        painter.rect_filled(
            response.rect,
            Rounding::ZERO,
            self.config.theme.canvas_colors.background,
        );

        response
            .drag_started_by(egui::PointerButton::Middle)
            .then(|| msgs.push(Message::SetDragStart(pointer_pos_canvas)));

        vcd.draw_cursor(
            &self.config.theme,
            &mut painter,
            response.rect.size(),
            to_screen,
        );

        let clock_edges = vec![];

        let mut ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            // This 0.5 is very odd, but it fixes the lines we draw being smushed out across two
            // pixels, resulting in dimmer colors https://github.com/emilk/egui/issues/1322
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y) + Vec2::new(0.5, 0.5)),
            theme: &self.config.theme,
        };

        self.draw_mouse_gesture_widget(vcd, pointer_pos_canvas, &response, msgs, &mut ctx);

        let draw_clock_edges = match clock_edges.as_slice() {
            [] => false,
            [_single] => true,
            [first, second, ..] => second - first > 15.,
        };

        if draw_clock_edges {
            for clock_edge in clock_edges {
                self.draw_clock_edge(clock_edge, &mut ctx);
            }
        }

        if let Some(draw_commands) = &*self.draw_commands.borrow() {
            for drawing_info in signal_offsets {
                let color = *vcd
                    .signals
                    .get(drawing_info.signal_list_idx())
                    .and_then(|signal| signal.color())
                    .and_then(|color| self.config.theme.colors.get(&color))
                    .unwrap_or(&self.config.theme.signal_default);
                // We draw in absolute coords, but the signal offset in the y
                // direction is also in absolute coordinates, so we need to
                // compensate for that
                let y_offset = drawing_info.offset() - to_screen.transform_pos(Pos2::ZERO).y;
                match drawing_info {
                    ItemDrawingInfo::Signal(drawing_info) => {
                        if let Some(commands) = draw_commands.get(&drawing_info.tidx) {
                            for (old, new) in
                                commands.values.iter().zip(commands.values.iter().skip(1))
                            {
                                if commands.is_bool {
                                    self.draw_bool_transition((old, new), color, y_offset, &mut ctx)
                                } else {
                                    self.draw_region((old, new), color, y_offset, &mut ctx)
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn draw_gesture_line(
        &self,
        start: Pos2,
        end: Pos2,
        text: &str,
        active: bool,
        ctx: &mut DrawingContext,
    ) {
        let stroke = Stroke {
            color: if active {
                self.config.theme.gesture.color
            } else {
                self.config.theme.gesture.color.gamma_multiply(0.3)
            },
            width: self.config.theme.gesture.width,
        };
        ctx.painter.line_segment(
            [
                (ctx.to_screen)(end.x, end.y),
                (ctx.to_screen)(start.x, start.y),
            ],
            stroke,
        );
        ctx.painter.text(
            (ctx.to_screen)(end.x, end.y),
            Align2::LEFT_CENTER,
            text.to_string(),
            FontId::default(),
            self.config.theme.foreground,
        );
    }

    fn draw_mouse_gesture_widget(
        &self,
        vcd: &VcdData,
        pointer_pos_canvas: Option<Pos2>,
        response: &egui::Response,
        msgs: &mut Vec<Message>,
        ctx: &mut DrawingContext,
    ) {
        let frame_width = response.rect.width();
        if let Some(start_location) = self.gesture_start_location {
            response.dragged_by(egui::PointerButton::Middle).then(|| {
                let current_location = pointer_pos_canvas.unwrap();
                match gesture_type(start_location, current_location) {
                    Some(GestureKind::ZoomToFit) => self.draw_gesture_line(
                        start_location,
                        current_location,
                        "Zoom to fit",
                        true,
                        ctx,
                    ),
                    Some(GestureKind::ZoomIn) => {
                        let stroke = Stroke {
                            color: self.config.theme.gesture.color,
                            width: self.config.theme.gesture.width,
                        };
                        let startx = start_location.x;
                        let starty = start_location.y;
                        let endx = current_location.x;
                        let height = response.rect.size().y;
                        ctx.painter.line_segment(
                            [
                                (ctx.to_screen)(startx, 0.0),
                                (ctx.to_screen)(startx, height),
                            ],
                            stroke,
                        );
                        ctx.painter.line_segment(
                            [(ctx.to_screen)(endx, 0.0), (ctx.to_screen)(endx, height)],
                            stroke,
                        );
                        ctx.painter.line_segment(
                            [
                                (ctx.to_screen)(start_location.x, start_location.y),
                                (ctx.to_screen)(endx, starty),
                            ],
                            stroke,
                        );
                        let (minx, maxx) = if endx < startx {
                            (endx, startx)
                        } else {
                            (startx, endx)
                        };
                        ctx.painter.text(
                            (ctx.to_screen)(current_location.x, current_location.y),
                            Align2::LEFT_CENTER,
                            format!(
                                "Zoom in: {} to {}",
                                time_string(
                                    &(vcd
                                        .viewport
                                        .to_time(minx as f64, frame_width)
                                        .round()
                                        .to_integer()),
                                    &vcd.inner.metadata,
                                    &(self.wanted_timescale)
                                ),
                                time_string(
                                    &(vcd
                                        .viewport
                                        .to_time(maxx as f64, frame_width)
                                        .round()
                                        .to_integer()),
                                    &vcd.inner.metadata,
                                    &(self.wanted_timescale)
                                ),
                            ),
                            FontId::default(),
                            self.config.theme.foreground,
                        );
                    }
                    Some(GestureKind::ScrollToStart) => {
                        self.draw_gesture_line(
                            start_location,
                            current_location,
                            "Scroll to start",
                            true,
                            ctx,
                        );
                    }
                    Some(GestureKind::ScrollToEnd) => {
                        self.draw_gesture_line(
                            start_location,
                            current_location,
                            "Scroll to end",
                            true,
                            ctx,
                        );
                    }
                    Some(GestureKind::ZoomOut) => {
                        self.draw_gesture_line(
                            start_location,
                            current_location,
                            "Zoom out",
                            true,
                            ctx,
                        );
                    }
                    _ => {
                        self.draw_gesture_line(start_location, current_location, "", false, ctx);
                    }
                }
            });

            response
                .drag_released_by(egui::PointerButton::Middle)
                .then(|| {
                    let end_location = pointer_pos_canvas.unwrap();
                    match gesture_type(start_location, end_location) {
                        Some(GestureKind::ZoomToFit) => {
                            msgs.push(Message::ZoomToFit);
                        }
                        Some(GestureKind::ZoomIn) => {
                            let (minx, maxx) = if end_location.x < start_location.x {
                                (end_location.x, start_location.x)
                            } else {
                                (start_location.x, end_location.x)
                            };
                            msgs.push(Message::ZoomToRange {
                                start: vcd
                                    .viewport
                                    .to_time(minx as f64, frame_width)
                                    .to_f64()
                                    .unwrap(),
                                end: vcd
                                    .viewport
                                    .to_time(maxx as f64, frame_width)
                                    .to_f64()
                                    .unwrap(),
                            })
                        }
                        Some(GestureKind::ScrollToStart) => {
                            msgs.push(Message::ScrollToStart);
                        }
                        Some(GestureKind::ScrollToEnd) => {
                            msgs.push(Message::ScrollToEnd);
                        }
                        Some(GestureKind::ZoomOut) => {
                            msgs.push(Message::CanvasZoom {
                                mouse_ptr_timestamp: None,
                                delta: 2.0,
                            });
                        }
                        _ => {}
                    }
                    msgs.push(Message::SetDragStart(None))
                });
        };
    }

    fn draw_region(
        &self,
        ((old_x, prev_region), (new_x, _)): (&(f32, DrawnRegion), &(f32, DrawnRegion)),
        user_color: Color32,
        offset: f32,
        ctx: &mut DrawingContext,
    ) {
        if let Some((prev_value, color)) = &prev_region.inner {
            let stroke = Stroke {
                color: color.color(user_color, ctx.theme),
                width: self.config.theme.linewidth,
            };

            let transition_width = (new_x - old_x).min(6.) as f32;

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

            let text_size = ctx.cfg.line_height - 5.;
            let char_width = text_size * (20. / 31.);

            let text_area = (new_x - old_x) as f32 - transition_width;
            let num_chars = (text_area / char_width).floor();
            let fits_text = num_chars >= 1.;

            if fits_text {
                let content = if prev_value.len() > num_chars as usize {
                    prev_value
                        .chars()
                        .take(num_chars as usize - 1)
                        .chain(['…'].into_iter())
                        .collect::<String>()
                } else {
                    prev_value.to_string()
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
        color: Color32,
        offset: f32,
        ctx: &mut DrawingContext,
    ) {
        if let (Some((prev_value, prev_kind)), Some((new_value, new_kind))) =
            (&prev_region.inner, &new_region.inner)
        {
            let trace_coords = |x, y| (ctx.to_screen)(x, y * ctx.cfg.line_height + offset);

            let (old_height, old_color, old_bg) =
                prev_value.bool_drawing_spec(color, &self.config.theme, *prev_kind);
            let (new_height, _, _) =
                new_value.bool_drawing_spec(color, &self.config.theme, *new_kind);

            let stroke = Stroke {
                color: old_color,
                width: self.config.theme.linewidth,
            };

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

    fn draw_clock_edge(&self, x_pos: f32, ctx: &mut DrawingContext) {
        let Pos2 {
            x: x_pos,
            y: y_start,
        } = (ctx.to_screen)(x_pos, 0.);
        ctx.painter.vline(
            x_pos,
            (y_start)..=(y_start + ctx.cfg.canvas_height),
            Stroke {
                color: self.config.theme.signal_highimp.gamma_multiply(0.7),
                width: 2.,
            },
        );
    }
}

struct DrawingContext<'a> {
    painter: &'a mut Painter,
    cfg: &'a DrawConfig,
    to_screen: &'a dyn Fn(f32, f32) -> Pos2,
    theme: &'a SurferTheme,
}

impl VcdData {
    fn draw_cursor(
        &self,
        theme: &SurferTheme,
        painter: &mut Painter,
        size: Vec2,
        to_screen: RectTransform,
    ) {
        if let Some(cursor) = &self.cursor {
            let x = self.viewport.from_time(cursor, size.x as f64);

            let stroke = Stroke {
                color: theme.cursor.color,
                width: theme.cursor.width,
            };
            painter.line_segment(
                [
                    to_screen.transform_pos(Pos2::new(x as f32 + 0.5, 0.)),
                    to_screen.transform_pos(Pos2::new(x as f32 + 0.5, size.y)),
                ],
                stroke,
            )
        }
    }
}

pub struct DrawConfig {
    canvas_height: f32,
    line_height: f32,
    max_transition_width: i32,
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
            ValueKind::Custom(custom_color) => custom_color.clone(),
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

fn gesture_type(start_location: Pos2, end_location: Pos2) -> Option<GestureKind> {
    let tan225 = 0.41421356237;
    let delta = end_location - start_location;

    if delta.x < 0.0 {
        if delta.y.abs() < -tan225 * delta.x {
            // West
            Some(GestureKind::ZoomIn)
        } else if delta.y < 0.0 && delta.x < delta.y * tan225 {
            // North west
            Some(GestureKind::ZoomToFit)
        } else if delta.y > 0.0 && delta.x < -delta.y * tan225 {
            // South west
            Some(GestureKind::ScrollToStart)
        // } else if delta.y < 0.0 {
        //    // North
        //    None
        } else {
            // South
            None
        }
    } else {
        if delta.x * tan225 > delta.y.abs() {
            // East
            Some(GestureKind::ZoomIn)
        } else if delta.y < 0.0 && delta.x > -delta.y * tan225 {
            // North east
            Some(GestureKind::ZoomOut)
        } else if delta.y > 0.0 && delta.x > delta.y * tan225 {
            // South east
            Some(GestureKind::ScrollToEnd)
        // } else if delta.y > 0.0 {
        //    // North
        //    None
        } else {
            // South
            None
        }
    }
}
