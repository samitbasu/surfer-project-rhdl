use eframe::egui::{Context, Grid, RichText, WidgetText, Window};
use eframe::emath::{Align2, Pos2, Rect, Vec2};
use eframe::epaint::{FontId, Rounding, Stroke};
use itertools::Itertools;
use num::BigInt;

use crate::State;
use crate::{
    config::SurferTheme,
    displayed_item::{DisplayedCursor, DisplayedItem},
    message::Message,
    time::time_string,
    view::{DrawingContext, ItemDrawingInfo},
    viewport::Viewport,
    wave_data::WaveData,
};

pub const DEFAULT_CURSOR_NAME: &str = "Cursor";

impl WaveData {
    pub fn draw_cursor(
        &self,
        theme: &SurferTheme,
        ctx: &mut DrawingContext,
        size: Vec2,
        viewport: &Viewport,
    ) {
        if let Some(cursor) = &self.cursor {
            let x = viewport.from_time(cursor, size.x);

            let stroke = Stroke {
                color: theme.cursor.color,
                width: theme.cursor.width,
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

    pub fn draw_cursors(
        &self,
        theme: &SurferTheme,
        ctx: &mut DrawingContext,
        size: Vec2,
        viewport: &Viewport,
    ) {
        for (idx, cursor) in &self.cursors {
            let color = self
                .displayed_items
                .iter()
                .find_map(|item| match item {
                    DisplayedItem::Cursor(tmp_cursor) => {
                        if *idx == tmp_cursor.idx {
                            Some(tmp_cursor)
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .and_then(|displayed_cursor| displayed_cursor.color.clone())
                .and_then(|color| theme.colors.get(&color))
                .unwrap_or(&theme.cursor.color);
            let stroke = Stroke {
                color: *color,
                width: theme.cursor.width,
            };
            let x = viewport.from_time(cursor, size.x);
            ctx.painter.line_segment(
                [
                    (ctx.to_screen)(x + 0.5, -0.5),
                    (ctx.to_screen)(x + 0.5, size.y),
                ],
                stroke,
            )
        }
    }

    pub fn set_cursor_position(&mut self, idx: u8) {
        let Some(location) = &self.cursor else {
            return;
        };
        if self
            .displayed_items
            .iter()
            .find_map(|item| match item {
                DisplayedItem::Cursor(cursor) => {
                    if cursor.idx == idx {
                        Some(cursor)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .is_none()
        {
            let cursor = DisplayedCursor {
                color: None,
                background_color: None,
                name: None,
                idx,
            };
            self.displayed_items.push(DisplayedItem::Cursor(cursor));
        }
        self.cursors.insert(idx, location.clone());
    }

    pub fn draw_cursor_number_boxes(
        &self,
        ctx: &mut DrawingContext,
        size: Vec2,
        theme: &SurferTheme,
        viewport: &Viewport,
    ) {
        let text_size = ctx.cfg.text_size;

        for displayed_item in self.displayed_items.iter().filter_map(|item| match item {
            DisplayedItem::Cursor(cursor) => Some(cursor),
            _ => None,
        }) {
            let background_color = displayed_item
                .color
                .as_ref()
                .and_then(|color| theme.colors.get(color))
                .unwrap_or(&theme.cursor.color);

            let x = self.numbered_cursor_location(displayed_item.idx, viewport, size.x);

            let idx_string = displayed_item.idx.to_string();
            // Determine size of text
            let rect = ctx.painter.text(
                (ctx.to_screen)(x, size.y * 0.5),
                Align2::CENTER_CENTER,
                idx_string.clone(),
                FontId::proportional(text_size),
                theme.foreground,
            );

            // Background rectangle
            let min = (ctx.to_screen)(rect.min.x, 0.);
            let max = (ctx.to_screen)(rect.max.x, size.y);
            let min = Pos2::new(rect.min.x - 2., min.y);
            let max = Pos2::new(rect.max.x + 2., max.y);

            ctx.painter
                .rect_filled(Rect { min, max }, Rounding::default(), *background_color);

            // Draw actual text on top of rectangle
            ctx.painter.text(
                (ctx.to_screen)(x, size.y * 0.5),
                Align2::CENTER_CENTER,
                idx_string,
                FontId::proportional(text_size),
                theme.foreground,
            );
        }
    }
}

impl State {
    pub fn draw_cursor_window(&self, waves: &WaveData, ctx: &Context, msgs: &mut Vec<Message>) {
        let mut open = true;

        let mut cursors: Vec<(u8, &BigInt, WidgetText)> = vec![];
        if let Some(cursor) = &waves.cursor {
            cursors.push((255, cursor, WidgetText::RichText(RichText::new("Primary"))))
        }

        let mut numbered_cursors = waves
            .displayed_items
            .iter()
            .filter_map(|displayed_item| match displayed_item {
                DisplayedItem::Cursor(cursor) => {
                    let text_color = displayed_item
                        .color()
                        .and_then(|color| self.config.theme.colors.get(&color))
                        .unwrap_or(&self.config.theme.foreground);

                    Some((
                        cursor.idx,
                        waves.numbered_cursor_time(cursor.idx),
                        cursor.cursor_text(text_color),
                    ))
                }
                _ => None,
            })
            .sorted_by(|a, b| Ord::cmp(&a.0, &b.0))
            .collect_vec();

        cursors.append(&mut numbered_cursors);
        Window::new("Cursors")
            .collapsible(true)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    Grid::new("cursors")
                        .striped(true)
                        .num_columns(cursors.len() + 1)
                        .spacing([5., 5.])
                        .show(ui, |ui| {
                            ui.label("");
                            for (cursor_idx, _, widget_text) in &cursors {
                                if *cursor_idx < 255 {
                                    ui.selectable_label(false, widget_text.clone())
                                        .clicked()
                                        .then(|| {
                                            msgs.push(Message::GoToCursorPosition(*cursor_idx, 0))
                                        });
                                } else {
                                    ui.selectable_label(false, widget_text.clone())
                                        .clicked()
                                        .then(|| {
                                            msgs.push(Message::GoToTime(waves.cursor.clone(), 0))
                                        });
                                }
                            }
                            ui.end_row();
                            for (cursor_idx, row_cursor_time, row_widget_text) in &cursors {
                                if *cursor_idx < 255 {
                                    ui.selectable_label(false, row_widget_text.clone())
                                        .clicked()
                                        .then(|| {
                                            msgs.push(Message::GoToCursorPosition(*cursor_idx, 0))
                                        });
                                } else {
                                    ui.selectable_label(false, row_widget_text.clone())
                                        .clicked()
                                        .then(|| {
                                            msgs.push(Message::GoToTime(waves.cursor.clone(), 0))
                                        });
                                }
                                for (_, col_cursor_time, _) in &cursors {
                                    ui.label(time_string(
                                        &(*row_cursor_time - *col_cursor_time),
                                        &waves.inner.metadata().timescale,
                                        &self.wanted_timeunit,
                                        &self.get_time_format(),
                                    ));
                                }
                                ui.end_row();
                            }
                        });
                    ui.add_space(15.);
                    if ui.button("Close").clicked() {
                        msgs.push(Message::SetCursorWindowVisible(false))
                    }
                });
            });
        if !open {
            msgs.push(Message::SetCursorWindowVisible(false))
        }
    }

    pub fn draw_cursor_boxes(
        &self,
        waves: &WaveData,
        ctx: &mut DrawingContext,
        item_offsets: &[ItemDrawingInfo],
        view_width: f32,
        gap: f32,
        viewport: &Viewport,
    ) {
        let text_size = ctx.cfg.text_size;

        for drawing_info in item_offsets.iter().filter_map(|item| match item {
            ItemDrawingInfo::Cursor(cursor) => Some(cursor),
            _ => None,
        }) {
            let Some(item) = waves.displayed_items.get(drawing_info.item_list_idx) else {
                return;
            };

            // We draw in absolute coords, but the variable offset in the y
            // direction is also in absolute coordinates, so we need to
            // compensate for that
            let y_offset = drawing_info.top - (ctx.to_screen)(0., 0.).y;

            let background_color = item
                .color()
                .and_then(|color| self.config.theme.colors.get(&color))
                .unwrap_or(&self.config.theme.cursor.color);

            let x = waves.numbered_cursor_location(drawing_info.idx, viewport, view_width);

            // Time string
            let time = time_string(
                waves
                    .cursors
                    .get(&drawing_info.idx)
                    .unwrap_or(&BigInt::from(0)),
                &waves.inner.metadata().timescale,
                &self.wanted_timeunit,
                &self.get_time_format(),
            );

            // Determine size of text
            let rect = ctx.painter.text(
                (ctx.to_screen)(x, y_offset),
                Align2::CENTER_TOP,
                time.clone(),
                FontId::proportional(text_size),
                self.config.theme.foreground,
            );
            // Background rectangle
            let min = (ctx.to_screen)(rect.min.x, y_offset - gap);
            let max = (ctx.to_screen)(rect.max.x, y_offset + ctx.cfg.line_height + gap);
            let min = Pos2::new(rect.min.x - gap, min.y);
            let max = Pos2::new(rect.max.x + gap, max.y);

            ctx.painter
                .rect_filled(Rect { min, max }, Rounding::default(), *background_color);

            // Draw actual text on top of rectangle
            ctx.painter.text(
                (ctx.to_screen)(x, y_offset),
                Align2::CENTER_TOP,
                time,
                FontId::proportional(text_size),
                self.config.theme.foreground,
            );
        }
    }
}
