use color_eyre::eyre::WrapErr;
use ecolor::Color32;
use egui::{PointerButton, Response, Rgba, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use emath::{Align2, Pos2, Rect, RectTransform, Vec2};
use epaint::{CubicBezierShape, FontId, PathShape, PathStroke, RectShape, Rounding, Shape, Stroke};
use ftr_parser::types::{Transaction, TxGenerator};
use itertools::Itertools;
use log::{error, warn};
use num::bigint::{ToBigInt, ToBigUint};
use num::{BigInt, BigUint, ToPrimitive};
use rayon::prelude::{IntoParallelRefIterator, ParallelBridge, ParallelIterator};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::f64::consts::PI;
use surfer_translation_types::{
    SubFieldFlatTranslationResult, TranslatedValue, ValueKind, VariableInfo, VariableType,
};

use crate::clock_highlighting::draw_clock_edge;
use crate::config::SurferTheme;
use crate::data_container::DataContainer;
use crate::displayed_item::{
    DisplayedFieldRef, DisplayedItemIndex, DisplayedItemRef, DisplayedVariable,
};
use crate::transaction_container::{TransactionRef, TransactionStreamRef};
use crate::translation::{TranslationResultExt, TranslatorList, ValueKindExt, VariableInfoExt};
use crate::view::{DrawConfig, DrawingContext, ItemDrawingInfo};
use crate::viewport::Viewport;
use crate::wave_container::{QueryResult, VariableRefExt};
use crate::wave_data::WaveData;
use crate::CachedDrawData::TransactionDrawData;
use crate::{
    displayed_item::DisplayedItem, CachedDrawData, CachedTransactionDrawData, CachedWaveDrawData,
    Message, State,
};

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
    is_clock: bool,
    values: Vec<(f32, DrawnRegion)>,
}

impl DrawingCommands {
    pub fn new_bool() -> Self {
        Self {
            values: vec![],
            is_bool: true,
            is_clock: false,
        }
    }

    pub fn new_clock() -> Self {
        Self {
            values: vec![],
            is_bool: true,
            is_clock: true,
        }
    }

    pub fn new_wide() -> Self {
        Self {
            values: vec![],
            is_bool: false,
            is_clock: false,
        }
    }

    pub fn push(&mut self, val: (f32, DrawnRegion)) {
        self.values.push(val);
    }
}

pub struct TxDrawingCommands {
    min: Pos2,
    max: Pos2,
    gen_ref: TransactionStreamRef, // makes it easier to later access the actual Transaction object
}

struct VariableDrawCommands {
    clock_edges: Vec<f32>,
    display_id: DisplayedItemRef,
    local_commands: HashMap<Vec<String>, DrawingCommands>,
    local_msgs: Vec<Message>,
}

fn variable_draw_commands(
    displayed_variable: &DisplayedVariable,
    display_id: DisplayedItemRef,
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
        .as_waves()
        .unwrap()
        .variable_meta(&displayed_variable.variable_ref)
        .context("failed to get variable meta")
    {
        Ok(meta) => meta,
        Err(e) => {
            warn!("{e:#?}");
            return None;
        }
    };

    let displayed_field_ref: DisplayedFieldRef = display_id.into();
    let translator = waves.variable_translator(&displayed_field_ref, translators);
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
            .as_waves()
            .unwrap()
            .query_variable(&displayed_variable.variable_ref, time);
        next_change = match &query_result {
            Ok(Some(QueryResult {
                next: Some(timestamp),
                ..
            })) => waves.viewports[viewport_idx].pixel_from_time(
                &timestamp.to_bigint().unwrap(),
                view_width,
                &waves.num_timestamps(),
            ),
            // If we don't have a next timestamp, we don't need to recheck until the last time
            // step
            Ok(_) => timestamps.last().map(|t| t.0).unwrap_or_default(),
            // If we get an error here, we'll let the next match block handle it, but we'll take
            // note that we need to recheck every pixel until the end
            _ => timestamps.first().map(|t| t.0).unwrap_or_default(),
        };

        let (change_time, val) = match query_result {
            Ok(Some(QueryResult {
                current: Some((change_time, val)),
                ..
            })) => (change_time, val),
            Ok(Some(QueryResult { current: None, .. })) | Ok(None) => continue,
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
                    "{translator_name} for {variable_name} failed. Disabling:",
                    translator_name = translator.name(),
                    variable_name = displayed_variable.variable_ref.full_path_string()
                );
                error!("{e:#}");
                local_msgs.push(Message::ResetVariableFormat(displayed_field_ref));
                return None;
            }
        };

        let fields = translation_result.format_flat(
            &displayed_variable.format,
            &displayed_variable.field_formats,
            translators,
        );

        for SubFieldFlatTranslationResult { names, value } in fields {
            let entry = local_commands.entry(names.clone()).or_insert_with(|| {
                match info.get_subinfo(&names) {
                    VariableInfo::Bool => DrawingCommands::new_bool(),
                    VariableInfo::Clock => DrawingCommands::new_clock(),
                    _ => DrawingCommands::new_wide(),
                }
            });

            let prev = prev_values.get(&names);

            // If the value changed between this and the previous pixel, we want to
            // draw a transition even if the translated value didn't change.  We
            // only want to do this for root variables, because resolving when a
            // sub-field change is tricky without more information from the
            // translators
            let anti_alias = &change_time > prev_time
                && names.is_empty()
                && waves.inner.as_waves().unwrap().wants_anti_aliasing();
            let new_value = prev != Some(&value);

            // This is not the value we drew last time
            if new_value || is_last_timestep || anti_alias {
                prev_values
                    .entry(names.clone())
                    .or_insert(value.clone())
                    .clone_from(&value);

                if let VariableInfo::Clock = info.get_subinfo(&names) {
                    match value.as_ref().map(|result| result.value.as_str()) {
                        Some("1") => {
                            if !is_last_timestep && !is_first_timestep {
                                clock_edges.push(*pixel);
                            }
                        }
                        Some(_) => {}
                        None => {}
                    }
                }

                entry.push((
                    *pixel,
                    DrawnRegion {
                        inner: value,
                        force_anti_alias: anti_alias && !new_value,
                    },
                ));
            }
        }
    }
    Some(VariableDrawCommands {
        clock_edges,
        display_id,
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
        if let Some(waves) = &self.waves {
            let draw_data = match waves.inner {
                DataContainer::Waves(_) => {
                    self.generate_wave_draw_commands(waves, cfg, frame_width, msgs, viewport_idx)
                }
                DataContainer::Transactions(_) => self.generate_transaction_draw_commands(
                    waves,
                    cfg,
                    frame_width,
                    msgs,
                    viewport_idx,
                ),
                DataContainer::Empty => None,
            };
            self.sys.draw_data.borrow_mut()[viewport_idx] = draw_data;
        }
        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().end("Generate draw commands");
    }

    fn generate_wave_draw_commands(
        &self,
        waves: &WaveData,
        cfg: &DrawConfig,
        frame_width: f32,
        msgs: &mut Vec<Message>,
        viewport_idx: usize,
    ) -> Option<CachedDrawData> {
        let mut draw_commands = HashMap::new();

        let num_timestamps = waves.num_timestamps().clone();
        let max_time = num_timestamps.to_f64().unwrap_or(f64::MAX);
        let mut clock_edges = vec![];
        // Compute which timestamp to draw in each pixel. We'll draw from -transition_width to
        // width + transition_width in order to draw initial transitions outside the screen
        let mut timestamps = (-cfg.max_transition_width
            ..(frame_width as i32 + cfg.max_transition_width))
            .par_bridge()
            .filter_map(|x| {
                let time = waves.viewports[viewport_idx]
                    .as_absolute_time(x as f64, frame_width, &num_timestamps)
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
            .map(|id| (*id, waves.displayed_items.get(id)))
            .filter_map(|(id, item)| match item {
                Some(DisplayedItem::Variable(variable_ref)) => Some((id, variable_ref)),
                _ => None,
            })
            // Iterate over the variables, generating draw commands for all the
            // subfields
            .filter_map(|(id, displayed_variable)| {
                variable_draw_commands(
                    displayed_variable,
                    id,
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
            display_id,
            local_commands,
            mut local_msgs,
        } in commands
        {
            msgs.append(&mut local_msgs);
            for (field, val) in local_commands {
                draw_commands.insert(
                    DisplayedFieldRef {
                        item: display_id,
                        field,
                    },
                    val,
                );
            }
            clock_edges.append(&mut new_clock_edges);
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

        Some(CachedDrawData::WaveDrawData(CachedWaveDrawData {
            draw_commands,
            clock_edges,
            ticks,
        }))
    }

    fn generate_transaction_draw_commands(
        &self,
        waves: &WaveData,
        cfg: &DrawConfig,
        frame_width: f32,
        msgs: &mut Vec<Message>,
        viewport_idx: usize,
    ) -> Option<CachedDrawData> {
        let mut draw_commands = HashMap::new();
        let mut stream_to_displayed_txs = HashMap::new();
        let mut inc_relation_tx_ids = vec![];
        let mut out_relation_tx_ids = vec![];

        let (focused_tx_ref, old_focused_tx) = &waves.focused_transaction;
        let mut new_focused_tx: Option<&Transaction> = None;

        let viewport = waves.viewports[viewport_idx];

        let displayed_streams = waves
            .displayed_items_order
            .par_iter()
            .map(|id| waves.displayed_items.get(id))
            .filter_map(|item| match item {
                Some(DisplayedItem::Stream(stream_ref)) => Some(stream_ref),
                _ => None,
            })
            .collect::<Vec<_>>();

        for displayed_stream in displayed_streams {
            let tx_stream_ref = &displayed_stream.transaction_stream_ref;

            let mut generators: Vec<&TxGenerator> = vec![];
            let mut displayed_transactions = vec![];

            if tx_stream_ref.is_stream() {
                let stream = waves
                    .inner
                    .as_transactions()
                    .unwrap()
                    .get_stream(tx_stream_ref.stream_id)
                    .unwrap();

                for gen_id in &stream.generators {
                    generators.push(
                        waves
                            .inner
                            .as_transactions()
                            .unwrap()
                            .get_generator(*gen_id)
                            .unwrap(),
                    );
                }
            } else {
                generators.push(
                    waves
                        .inner
                        .as_transactions()
                        .unwrap()
                        .get_generator(tx_stream_ref.gen_id.unwrap())
                        .unwrap(),
                );
            }

            let mut last_times_on_row = vec![(BigUint::ZERO, BigUint::ZERO)];
            for gen in &generators {
                for tx in &gen.transactions {
                    let mut curr_row = 0;
                    let start_time = tx.get_start_time();
                    let end_time = tx.get_end_time();
                    let curr_tx_id = tx.get_tx_id();

                    while start_time > last_times_on_row[curr_row].0
                        && start_time < last_times_on_row[curr_row].1
                    {
                        curr_row += 1;
                        if last_times_on_row.len() <= curr_row {
                            last_times_on_row.push((BigUint::ZERO, BigUint::ZERO));
                        }
                    }
                    last_times_on_row[curr_row] = (start_time.clone(), end_time.clone());

                    if end_time.to_f64().unwrap()
                        > viewport.curr_left.absolute(&waves.num_timestamps()).0
                        && start_time.to_f64().unwrap()
                            < viewport.curr_right.absolute(&waves.num_timestamps()).0
                    {
                        if let Some(focused_tx_ref) = focused_tx_ref {
                            if curr_tx_id == focused_tx_ref.id {
                                new_focused_tx = Some(tx);
                            }
                        }

                        displayed_transactions.push(TransactionRef { id: curr_tx_id });
                        let min = Pos2::new(
                            viewport.pixel_from_time(
                                &start_time.to_bigint().unwrap(),
                                frame_width - 1.,
                                &waves.num_timestamps(),
                            ),
                            cfg.line_height * curr_row as f32 + 4.0,
                        );
                        let max = Pos2::new(
                            viewport.pixel_from_time(
                                &end_time.to_bigint().unwrap(),
                                frame_width - 1.,
                                &waves.num_timestamps(),
                            ),
                            cfg.line_height * (curr_row + 1) as f32 - 4.0,
                        );

                        let tx_ref = TransactionRef { id: curr_tx_id };
                        draw_commands.insert(
                            tx_ref,
                            TxDrawingCommands {
                                min,
                                max,
                                gen_ref: TransactionStreamRef::new_gen(
                                    tx_stream_ref.stream_id,
                                    gen.id,
                                    gen.name.clone(),
                                ),
                            },
                        );
                    }
                }
            }
            stream_to_displayed_txs.insert(tx_stream_ref.clone(), displayed_transactions);
        }

        if let Some(focused_tx) = new_focused_tx {
            for rel in &focused_tx.inc_relations {
                inc_relation_tx_ids.push(TransactionRef {
                    id: rel.source_tx_id,
                });
            }
            for rel in &focused_tx.out_relations {
                out_relation_tx_ids.push(TransactionRef { id: rel.sink_tx_id });
            }
            if old_focused_tx.is_none() || Some(focused_tx) != old_focused_tx.as_ref() {
                msgs.push(Message::FocusTransaction(
                    focused_tx_ref.clone(),
                    Some(focused_tx.clone()),
                ));
            }
        }

        Some(TransactionDrawData(CachedTransactionDrawData {
            draw_commands,
            stream_to_displayed_txs,
            inc_relation_tx_ids,
            out_relation_tx_ids,
        }))
    }

    pub fn draw_items(&mut self, msgs: &mut Vec<Message>, ui: &mut Ui, viewport_idx: usize) {
        let (response, mut painter) =
            ui.allocate_painter(ui.available_size(), Sense::click_and_drag());

        let cfg = match self.waves.as_ref().unwrap().inner {
            DataContainer::Waves(_) => DrawConfig::new(
                response.rect.size().y,
                self.config.layout.waveforms_line_height,
                self.config.layout.waveforms_text_size,
            ),
            DataContainer::Transactions(_) => DrawConfig::new(
                response.rect.size().y,
                self.config.layout.transactions_line_height,
                self.config.layout.waveforms_text_size,
            ),
            DataContainer::Empty => return,
        };
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
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
            let mouse_ptr_pos = to_screen.inverse().transform_pos(pointer_pos);
            if scroll_delta != Vec2::ZERO {
                msgs.push(Message::CanvasScroll {
                    delta: ui.input(|i| i.smooth_scroll_delta),
                    viewport_idx,
                });
            }

            if ui.input(egui::InputState::zoom_delta) != 1. {
                let mouse_ptr = Some(waves.viewports[viewport_idx].as_time_bigint(
                    mouse_ptr_pos.x,
                    frame_width,
                    &waves.num_timestamps(),
                ));

                msgs.push(Message::CanvasZoom {
                    mouse_ptr,
                    delta: ui.input(egui::InputState::zoom_delta),
                    viewport_idx,
                });
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
                });
            }
        });

        if response.dragged_by(PointerButton::Primary)
            || response.clicked_by(PointerButton::Primary)
        {
            if let Some(snap_point) =
                self.snap_to_edge(pointer_pos_canvas, waves, frame_width, viewport_idx)
            {
                msgs.push(Message::CursorSet(snap_point));
            }
        }

        painter.rect_filled(
            response.rect,
            Rounding::ZERO,
            self.config.theme.canvas_colors.background,
        );

        response
            .drag_started_by(PointerButton::Middle)
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
            // Get background color
            let background_color =
                &self.get_background_color(waves, drawing_info, DisplayedItemIndex(idx));

            self.draw_background(
                drawing_info,
                y_zero,
                &ctx,
                gap,
                frame_width,
                background_color,
            );
        }

        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().start("Wave drawing");

        match &self.sys.draw_data.borrow()[viewport_idx] {
            Some(CachedDrawData::WaveDrawData(draw_data)) => {
                let clock_edges = &draw_data.clock_edges;
                let draw_commands = &draw_data.draw_commands;
                let draw_clock_edges = match clock_edges.as_slice() {
                    [] => false,
                    [_single] => true,
                    [first, second, ..] => second - first > 20.,
                };
                let draw_clock_rising_marker =
                    draw_clock_edges && self.config.theme.clock_rising_marker;
                let ticks = &draw_data.ticks;
                if !ticks.is_empty() && self.show_ticks() {
                    let stroke = Stroke {
                        color: self.config.theme.ticks.style.color,
                        width: self.config.theme.ticks.style.width,
                    };

                    for (_, x) in ticks {
                        waves.draw_tick_line(*x, &mut ctx, &stroke);
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
                for (idx, drawing_info) in waves.drawing_infos.iter().enumerate() {
                    // We draw in absolute coords, but the variable offset in the y
                    // direction is also in absolute coordinates, so we need to
                    // compensate for that
                    let y_offset = drawing_info.top() - zero_y;

                    let displayed_item = waves
                        .displayed_items_order
                        .get(drawing_info.item_list_idx())
                        .and_then(|id| waves.displayed_items.get(id));
                    let color = displayed_item
                        .and_then(super::displayed_item::DisplayedItem::color)
                        .and_then(|color| self.config.theme.get_color(&color));

                    match drawing_info {
                        ItemDrawingInfo::Variable(variable_info) => {
                            if let Some(commands) =
                                draw_commands.get(&variable_info.displayed_field_ref)
                            {
                                // Get background color and determine best text color
                                let background_color = self.get_background_color(
                                    waves,
                                    drawing_info,
                                    DisplayedItemIndex(idx),
                                );
                                let text_color =
                                    self.config.theme.get_best_text_color(&background_color);

                                let color = *color.unwrap_or_else(|| {
                                    if let Some(DisplayedItem::Variable(variable)) = displayed_item
                                    {
                                        waves
                                            .inner
                                            .as_waves()
                                            .unwrap()
                                            .variable_meta(&variable.variable_ref)
                                            .ok()
                                            .and_then(|meta| meta.variable_type)
                                            .and_then(|var_type| {
                                                if var_type == VariableType::VCDParameter {
                                                    Some(&self.config.theme.variable_parameter)
                                                } else {
                                                    None
                                                }
                                            })
                                            .unwrap_or(&self.config.theme.variable_default)
                                    } else {
                                        &self.config.theme.variable_default
                                    }
                                });
                                for (old, new) in
                                    commands.values.iter().zip(commands.values.iter().skip(1))
                                {
                                    if commands.is_bool {
                                        self.draw_bool_transition(
                                            (old, new),
                                            new.1.force_anti_alias,
                                            color,
                                            y_offset,
                                            commands.is_clock && draw_clock_rising_marker,
                                            &mut ctx,
                                        );
                                    } else {
                                        self.draw_region(
                                            (old, new),
                                            color,
                                            y_offset,
                                            &mut ctx,
                                            *text_color,
                                        );
                                    }
                                }
                            }
                        }
                        ItemDrawingInfo::Divider(_) => {}
                        ItemDrawingInfo::Marker(_) => {}
                        ItemDrawingInfo::TimeLine(_) => {
                            let text_color = color.unwrap_or(
                                // Get background color and determine best text color
                                self.config
                                    .theme
                                    .get_best_text_color(&self.get_background_color(
                                        waves,
                                        drawing_info,
                                        DisplayedItemIndex(idx),
                                    )),
                            );
                            waves.draw_ticks(
                                Some(text_color),
                                ticks,
                                &ctx,
                                y_offset,
                                Align2::CENTER_TOP,
                                &self.config,
                            );
                        }
                        ItemDrawingInfo::Stream(_) => {}
                    }
                }
            }
            Some(CachedDrawData::TransactionDrawData(draw_data)) => {
                let draw_commands = &draw_data.draw_commands;
                let stream_to_displayed_txs = &draw_data.stream_to_displayed_txs;
                let inc_relation_tx_ids = &draw_data.inc_relation_tx_ids;
                let out_relation_tx_ids = &draw_data.out_relation_tx_ids;
                let highlight_rgba = Rgba::from(self.config.theme.transaction_highlight);

                let mut inc_relation_starts = vec![];
                let mut out_relation_starts = vec![];
                let mut focused_transaction_start: Option<Pos2> = None;

                let ticks = &waves.get_ticks(
                    &waves.viewports[viewport_idx],
                    &waves.inner.metadata().timescale,
                    frame_width,
                    cfg.text_size,
                    &self.wanted_timeunit,
                    &self.get_time_format(),
                    &self.config,
                );

                if !ticks.is_empty() && self.show_ticks() {
                    let stroke = Stroke {
                        color: self.config.theme.ticks.style.color,
                        width: self.config.theme.ticks.style.width,
                    };

                    for (_, x) in ticks {
                        waves.draw_tick_line(*x, &mut ctx, &stroke);
                    }
                }

                let zero_y = to_screen.transform_pos(Pos2::ZERO).y;
                for (idx, drawing_info) in waves.drawing_infos.iter().enumerate() {
                    let y_offset = drawing_info.top() - zero_y;

                    let displayed_item = waves
                        .displayed_items_order
                        .get(drawing_info.item_list_idx())
                        .and_then(|id| waves.displayed_items.get(id));
                    let color = displayed_item
                        .and_then(super::displayed_item::DisplayedItem::color)
                        .and_then(|color| self.config.theme.get_color(&color));

                    match drawing_info {
                        ItemDrawingInfo::Stream(stream) => {
                            if let Some(tx_refs) =
                                stream_to_displayed_txs.get(&stream.transaction_stream_ref)
                            {
                                for tx_ref in tx_refs {
                                    if let Some(tx_draw_command) = draw_commands.get(tx_ref) {
                                        let mut min = tx_draw_command.min;
                                        let mut max = tx_draw_command.max;

                                        min.x = min.x.max(0.);
                                        max.x = max.x.min(frame_width - 1.);

                                        let min = (ctx.to_screen)(min.x, y_offset + min.y);
                                        let max = (ctx.to_screen)(max.x, y_offset + max.y);

                                        let start = Pos2::new(min.x, (min.y + max.y) / 2.);

                                        let is_transaction_focused = waves
                                            .focused_transaction
                                            .0
                                            .as_ref()
                                            .is_some_and(|t| t == tx_ref);

                                        if inc_relation_tx_ids.contains(tx_ref) {
                                            inc_relation_starts.push(start);
                                        } else if out_relation_tx_ids.contains(tx_ref) {
                                            out_relation_starts.push(start);
                                        } else if is_transaction_focused {
                                            focused_transaction_start = Some(start);
                                        }

                                        let transaction_rect = Rect { min, max };
                                        let mut response =
                                            ui.allocate_rect(transaction_rect, Sense::click());

                                        response = handle_transaction_tooltip(
                                            response,
                                            waves,
                                            &tx_draw_command.gen_ref,
                                            tx_ref,
                                        );

                                        if response.clicked() {
                                            msgs.push(Message::FocusTransaction(
                                                Some(tx_ref.clone()),
                                                None,
                                            ));
                                        }
                                        let tx_fill_color = if is_transaction_focused {
                                            Color32::from(
                                                Rgba::from(
                                                    color
                                                        .unwrap_or(
                                                            &self.config.theme.transaction_default,
                                                        )
                                                        .additive(),
                                                ) + highlight_rgba,
                                            )
                                        } else {
                                            color
                                                .unwrap_or(&self.config.theme.transaction_default)
                                                .gamma_multiply(0.6)
                                        };
                                        let stroke = Stroke::new(1.5, tx_fill_color.additive());
                                        ctx.painter.rect(
                                            transaction_rect,
                                            Rounding::same(5.0),
                                            tx_fill_color,
                                            stroke,
                                        );
                                    }
                                }
                                // Draws the surrounding border of the stream
                                let stroke = Stroke::new(1.5, Color32::LIGHT_YELLOW);
                                ctx.painter.rect_stroke(
                                    Rect {
                                        min: (ctx.to_screen)(container_rect.min.x, y_offset + 1.),
                                        max: (ctx.to_screen)(
                                            frame_width,
                                            (drawing_info.bottom()
                                                - to_screen.transform_pos(Pos2::ZERO).y)
                                                - 2.,
                                        ),
                                    },
                                    Rounding::ZERO,
                                    stroke,
                                );
                            }
                        }
                        ItemDrawingInfo::TimeLine(_) => {
                            let text_color = color.unwrap_or(
                                // Get background color and determine best text color
                                self.config
                                    .theme
                                    .get_best_text_color(&self.get_background_color(
                                        waves,
                                        drawing_info,
                                        DisplayedItemIndex(idx),
                                    )),
                            );
                            waves.draw_ticks(
                                Some(text_color),
                                ticks,
                                &ctx,
                                y_offset,
                                Align2::CENTER_TOP,
                                &self.config,
                            );
                        }
                        ItemDrawingInfo::Variable(_) => {}
                        ItemDrawingInfo::Divider(_) => {}
                        ItemDrawingInfo::Marker(_) => {}
                    }
                }

                // Draws the relations of the focused transaction
                if let Some(focused_pos) = focused_transaction_start {
                    let arrow_color = self.config.theme.relation_arrow;
                    for start_pos in inc_relation_starts {
                        self.draw_arrow(start_pos, focused_pos, 25., arrow_color, &ctx);
                    }

                    for end_pos in out_relation_starts {
                        self.draw_arrow(focused_pos, end_pos, 25., arrow_color, &ctx);
                    }
                }
            }
            None => {}
        }
        #[cfg(feature = "performance_plot")]
        self.sys.timing.borrow_mut().end("Wave drawing");

        waves.draw_graphics(
            &mut ctx,
            response.rect.size(),
            &waves.viewports[viewport_idx],
            &self.config.theme,
        );

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
        text_color: Color32,
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
            let num_chars = (text_area / char_width).floor() as usize;
            let fits_text = num_chars >= 1;

            if fits_text {
                let content = if prev_result.value.len() > num_chars {
                    prev_result
                        .value
                        .chars()
                        .take(num_chars - 1)
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
                    text_color,
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
        draw_clock_marker: bool,
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

            if draw_clock_marker && (old_height < new_height) {
                ctx.painter.add(PathShape::convex_polygon(
                    vec![
                        trace_coords(*new_x - 2.5, 0.6),
                        trace_coords(*new_x, 0.4),
                        trace_coords(*new_x + 2.5, 0.6),
                    ],
                    old_color,
                    stroke,
                ));
            }

            if let Some(old_bg) = old_bg {
                ctx.painter.add(RectShape::new(
                    Rect {
                        min: (ctx.to_screen)(*old_x, offset),
                        max: (ctx.to_screen)(*new_x, offset + ctx.cfg.line_height),
                    },
                    Rounding::ZERO,
                    old_bg,
                    Stroke::new(0.0, Color32::BLACK),
                ));
            }
        }
    }

    /// Draws a curvy arrow from `start` to `end`.
    fn draw_arrow(
        &self,
        start: Pos2,
        end: Pos2,
        arrowhead_angle: f64,
        color: Color32,
        ctx: &DrawingContext,
    ) {
        let mut anchor1 = Pos2::default();
        let mut anchor2 = Pos2::default();

        let x_diff = (end.x - start.x).max(100.);

        anchor1.x = start.x + (2. / 5.) * x_diff;
        anchor1.y = start.y;

        anchor2.x = end.x - (2. / 5.) * x_diff;
        anchor2.y = end.y;

        let stroke = PathStroke::new(1.3, color);

        ctx.painter.add(Shape::CubicBezier(CubicBezierShape {
            points: [start, anchor1, anchor2, end],
            closed: false,
            fill: Default::default(),
            stroke: stroke.clone(),
        }));

        self.draw_arrowheads(anchor2, end, arrowhead_angle, ctx, stroke);
    }

    /// Draws arrowheads for the vector going from `vec_start` to `vec_tip`.
    /// The `angle` has to be in degrees.
    fn draw_arrowheads(
        &self,
        vec_start: Pos2,
        vec_tip: Pos2,
        angle: f64, // given in degrees
        ctx: &DrawingContext,
        stroke: PathStroke,
    ) {
        // FIXME: should probably make scale a function parameter
        let scale = 8.;

        let vec_x = (vec_tip.x - vec_start.x) as f64;
        let vec_y = (vec_tip.y - vec_start.y) as f64;

        let alpha = 2. * PI / 360. * angle;

        // calculate the points of the new vector, which forms an angle of the given degrees with the given vector
        let vec_angled_x = vec_x * alpha.cos() + vec_y * alpha.sin();
        let vec_angled_y = -vec_x * alpha.sin() + vec_y * alpha.cos();

        // scale the new vector to be ´scale´ long
        let vec_angled_x =
            (1. / (vec_angled_y - vec_angled_x).powi(2).sqrt()) * vec_angled_x * scale;
        let vec_angled_y =
            (1. / (vec_angled_y - vec_angled_x).powi(2).sqrt()) * vec_angled_y * scale;

        let arrowhead_left_x = vec_tip.x - vec_angled_x as f32;
        let arrowhead_left_y = vec_tip.y - vec_angled_y as f32;

        let arrowhead_right_x = vec_tip.x + vec_angled_y as f32;
        let arrowhead_right_y = vec_tip.y - vec_angled_x as f32;

        ctx.painter.add(Shape::line_segment(
            [vec_tip, Pos2::new(arrowhead_left_x, arrowhead_left_y)],
            stroke.clone(),
        ));

        ctx.painter.add(Shape::line_segment(
            [vec_tip, Pos2::new(arrowhead_right_x, arrowhead_right_y)],
            stroke,
        ));
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
        let size = response.rect.size();
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
                        }};
                    }

                    for id in waves.markers.keys().sorted() {
                        ui.button(format!("{id}")).clicked().then(|| {
                            msgs.push(Message::SetMarker {
                                id: *id,
                                time: time.clone(),
                            });
                            close_menu!();
                        });
                    }
                    // At the moment we only support 255 markers, and the cursor is the 255th
                    if waves.markers.len() < 254 {
                        ui.button("New").clicked().then(|| {
                            // NOTE: Safe unwrap, we have at least one empty slot
                            let id = (0..254).find(|id| !waves.markers.contains_key(id)).unwrap();
                            msgs.push(Message::SetMarker { id, time });
                            close_menu!();
                        });
                    }
                });
            }
        });
    }

    /// Takes a pointer pos in the canvas and returns a position that is snapped to transitions
    /// if the cursor is close enough to any transition. If the cursor is on the canvas and no
    /// transitions are close enough for snapping, the raw point will be returned. If the cursor is
    /// off the canvas, `None` is returned
    fn snap_to_edge(
        &self,
        pointer_pos_canvas: Option<Pos2>,
        waves: &WaveData,
        frame_width: f32,
        viewport_idx: usize,
    ) -> Option<BigInt> {
        let pos = pointer_pos_canvas?;
        let viewport = &waves.viewports[viewport_idx];
        let timestamp = viewport.as_time_bigint(pos.x, frame_width, &waves.num_timestamps());
        if let Some(utimestamp) = timestamp.to_biguint() {
            if let Some(vidx) = waves.get_item_at_y(pos.y) {
                if let Some(id) = waves.displayed_items_order.get(vidx) {
                    if let DisplayedItem::Variable(variable) = &waves.displayed_items[id] {
                        if let Ok(Some(res)) = waves
                            .inner
                            .as_waves()
                            .unwrap()
                            .query_variable(&variable.variable_ref, &utimestamp)
                        {
                            let prev_time = if let Some(v) = res.current {
                                v.0.to_bigint().unwrap()
                            } else {
                                BigInt::ZERO
                            };
                            let next_time = &res.next.unwrap_or_default().to_bigint().unwrap();
                            let prev = viewport.pixel_from_time(
                                &prev_time,
                                frame_width,
                                &waves.num_timestamps(),
                            );
                            let next = viewport.pixel_from_time(
                                next_time,
                                frame_width,
                                &waves.num_timestamps(),
                            );
                            if (prev - pos.x).abs() < (next - pos.x).abs() {
                                if (prev - pos.x).abs() <= self.config.snap_distance {
                                    return Some(prev_time.clone());
                                }
                            } else if (next - pos.x).abs() <= self.config.snap_distance {
                                return Some(next_time.clone());
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
        let x = viewport.pixel_from_time(time, size.x, &waves.num_timestamps());

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
        );
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
                    (1., Some(color.gamma_multiply(theme.waveform_opacity)))
                }
            }
            (ValueKind::Normal, other) => {
                if other == "0" {
                    (0., None)
                } else {
                    (1., Some(color.gamma_multiply(theme.waveform_opacity)))
                }
            }
        };
        (height, color, background)
    }
}

fn handle_transaction_tooltip(
    response: Response,
    waves: &WaveData,
    gen_ref: &TransactionStreamRef,
    tx_ref: &TransactionRef,
) -> Response {
    let tx = waves
        .inner
        .as_transactions()
        .unwrap()
        .get_generator(gen_ref.gen_id.unwrap())
        .unwrap()
        .transactions
        .iter()
        .find(|transaction| transaction.get_tx_id() == tx_ref.id)
        .unwrap();

    let response = response.on_hover_text(transaction_tooltip_text(waves, tx));
    response.on_hover_ui(|ui| transaction_tooltip_table(ui, tx))
}

fn transaction_tooltip_text(waves: &WaveData, tx: &Transaction) -> String {
    let time_scale = waves.inner.as_transactions().unwrap().inner.time_scale;

    format!(
        "tx#{}: {}{} - {}{}\nType: {}",
        tx.event.tx_id,
        tx.event.start_time,
        time_scale,
        tx.event.end_time,
        time_scale,
        waves
            .inner
            .as_transactions()
            .unwrap()
            .get_generator(tx.get_gen_id())
            .unwrap()
            .name
            .clone(),
    )
}

fn transaction_tooltip_table(ui: &mut Ui, tx: &Transaction) {
    TableBuilder::new(ui)
        .column(Column::exact(80.))
        .column(Column::exact(80.))
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.heading("Attribute");
            });
            header.col(|ui| {
                ui.heading("Value");
            });
        })
        .body(|body| {
            let total_rows = tx.attributes.len();
            let attributes = &tx.attributes;
            body.rows(15., total_rows, |mut row| {
                let attribute = attributes.get(row.index()).unwrap();
                row.col(|ui| {
                    ui.label(attribute.name.clone());
                });
                row.col(|ui| {
                    ui.label(attribute.value());
                });
            });
        });
}
