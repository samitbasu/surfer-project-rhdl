use eframe::{
    emath::{Align2, RectTransform},
    epaint::{FontId, Pos2, Rect, Rounding, Stroke, Vec2},
};
use num::BigInt;

use crate::{
    config::{SurferConfig, SurferTheme},
    displayed_item::{DisplayedCursor, DisplayedItem},
    time::{time_string, TimeUnit},
    view::{DrawingContext, ItemDrawingInfo},
    wave_data::WaveData,
};

pub const DEFAULT_CURSOR_NAME: &str = "Cursor";

impl WaveData {
    pub fn draw_cursor(
        &self,
        theme: &SurferTheme,
        ctx: &mut DrawingContext,
        size: Vec2,
        to_screen: RectTransform,
        viewport_idx: usize,
    ) {
        if let Some(cursor) = &self.cursor {
            let x = self.viewports[viewport_idx].from_time(cursor, size.x as f64);

            let stroke = Stroke {
                color: theme.cursor.color,
                width: theme.cursor.width,
            };
            ctx.painter.line_segment(
                [
                    to_screen.transform_pos(Pos2::new(x as f32 + 0.5, 0.)),
                    to_screen.transform_pos(Pos2::new(x as f32 + 0.5, size.y)),
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
        to_screen: RectTransform,
        viewport_idx: usize,
    ) {
        let viewport = &self.viewports[viewport_idx];
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
            let x = viewport.from_time(cursor, size.x as f64);
            ctx.painter.line_segment(
                [
                    to_screen.transform_pos(Pos2::new(x as f32 + 0.5, 0.)),
                    to_screen.transform_pos(Pos2::new(x as f32 + 0.5, size.y)),
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

    pub fn draw_cursor_boxes(
        &self,
        ctx: &mut DrawingContext,
        item_offsets: &[ItemDrawingInfo],
        to_screen: RectTransform,
        size: Vec2,
        gap: f32,
        config: &SurferConfig,
        wanted_timeunit: TimeUnit,
        viewport_idx: usize,
    ) {
        let text_size = ctx.cfg.text_size;

        for drawing_info in item_offsets.iter().filter_map(|item| match item {
            ItemDrawingInfo::Cursor(cursor) => Some(cursor),
            _ => None,
        }) {
            let Some(item) = self.displayed_items.get(drawing_info.signal_list_idx) else {
                return;
            };

            // We draw in absolute coords, but the signal offset in the y
            // direction is also in absolute coordinates, so we need to
            // compensate for that
            let y_offset = drawing_info.offset - to_screen.transform_pos(Pos2::ZERO).y;

            let background_color = item
                .color()
                .and_then(|color| config.theme.colors.get(&color))
                .unwrap_or(&config.theme.cursor.color);

            let x = self.viewports[viewport_idx]
                .from_time(self.cursors.get(&drawing_info.idx).unwrap(), size.x as f64)
                as f32;

            // Time string
            let time = time_string(
                self.cursors
                    .get(&drawing_info.idx)
                    .unwrap_or(&BigInt::from(0)),
                &self.inner.metadata().timescale,
                &wanted_timeunit,
            );

            // Determine size of text
            let rect = ctx.painter.text(
                (ctx.to_screen)(x, y_offset),
                Align2::CENTER_TOP,
                time.clone(),
                FontId::proportional(text_size),
                config.theme.foreground,
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
                config.theme.foreground,
            );
        }
    }
}
