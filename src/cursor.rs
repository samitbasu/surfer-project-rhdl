use eframe::egui::{Context, Grid, RichText, WidgetText, Window};
use eframe::emath::{Align2, Pos2, Rect, RectTransform, Vec2};
use eframe::epaint::{FontId, Rounding, Stroke};
use itertools::Itertools;
use num::BigInt;

use crate::{
    config::{SurferConfig, SurferTheme},
    displayed_item::{DisplayedCursor, DisplayedItem},
    message::Message,
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
    ) {
        if let Some(cursor) = &self.cursor {
            let x = self.viewport.from_time(cursor, size.x as f64);

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
            let x = self.viewport.from_time(cursor, size.x as f64);
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

            let x = self
                .viewport
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

    pub fn draw_cursor_window(
        &self,
        ctx: &Context,
        msgs: &mut Vec<Message>,
        config: &SurferConfig,
        wanted_timeunit: TimeUnit,
    ) {
        let mut open = true;

        let mut cursors: Vec<(u8, BigInt, WidgetText)> = vec![];
        if let Some(cursor) = &self.cursor {
            cursors.push((
                255,
                cursor.clone(),
                WidgetText::RichText(RichText::new("Primary")),
            ))
        }

        let mut numbered_cursors = self
            .displayed_items
            .iter()
            .filter_map(|displayed_item| match displayed_item {
                DisplayedItem::Cursor(cursor) => {
                    let text_color = displayed_item
                        .color()
                        .and_then(|color| config.theme.colors.get(&color))
                        .unwrap_or(&config.theme.foreground);

                    Some((
                        cursor.idx,
                        self.cursors.get(&cursor.idx).unwrap().clone(),
                        displayed_item.widget_text(text_color),
                    ))
                }
                _ => None,
            })
            .sorted_by(|a, b| Ord::cmp(&b.0, &a.0))
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
                                            msgs.push(Message::GoToCursorPosition(*cursor_idx))
                                        });
                                } else {
                                    ui.label(widget_text.clone());
                                }
                            }
                            ui.end_row();
                            for (cursor_idx, row_cursor_time, row_widget_text) in &cursors {
                                if *cursor_idx < 255 {
                                    ui.selectable_label(false, row_widget_text.clone())
                                        .clicked()
                                        .then(|| {
                                            msgs.push(Message::GoToCursorPosition(*cursor_idx))
                                        });
                                } else {
                                    ui.label(row_widget_text.clone());
                                }
                                for (_, col_cursor_time, _) in &cursors {
                                    ui.label(time_string(
                                        &(row_cursor_time.clone() - col_cursor_time),
                                        &self.inner.metadata().timescale,
                                        &wanted_timeunit,
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
}
