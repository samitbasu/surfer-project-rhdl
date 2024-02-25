use eframe::egui::{Context, Grid, RichText, WidgetText, Window};
use eframe::emath::{Align2, Pos2, Rect, Vec2};
use eframe::epaint::{FontId, Rounding, Stroke};
use itertools::Itertools;
use num::BigInt;

use crate::State;
use crate::{
    config::SurferTheme,
    displayed_item::{DisplayedItem, DisplayedMarker},
    message::Message,
    time::time_string,
    view::{DrawingContext, ItemDrawingInfo},
    viewport::Viewport,
    wave_data::WaveData,
};

pub const DEFAULT_MARKER_NAME: &str = "Marker";

impl WaveData {
    pub fn draw_cursor(
        &self,
        theme: &SurferTheme,
        ctx: &mut DrawingContext,
        size: Vec2,
        viewport: &Viewport,
    ) {
        if let Some(marker) = &self.cursor {
            let x = viewport.from_time(marker, size.x);

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

    pub fn draw_markers(
        &self,
        theme: &SurferTheme,
        ctx: &mut DrawingContext,
        size: Vec2,
        viewport: &Viewport,
    ) {
        for (idx, marker) in &self.markers {
            let color = self
                .displayed_items
                .iter()
                .find_map(|item| match item {
                    DisplayedItem::Marker(tmp_marker) => {
                        if *idx == tmp_marker.idx {
                            Some(tmp_marker)
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .and_then(|displayed_maker| displayed_maker.color.clone())
                .and_then(|color| theme.colors.get(&color))
                .unwrap_or(&theme.cursor.color);
            let stroke = Stroke {
                color: *color,
                width: theme.cursor.width,
            };
            let x = viewport.from_time(marker, size.x);
            ctx.painter.line_segment(
                [
                    (ctx.to_screen)(x + 0.5, -0.5),
                    (ctx.to_screen)(x + 0.5, size.y),
                ],
                stroke,
            )
        }
    }

    /// Set the marker with the specified id to the location. If the marker doesn't exist already,
    /// it will be created
    pub fn set_marker_position(&mut self, idx: u8, location: &BigInt) {
        if self
            .displayed_items
            .iter()
            .find_map(|item| match item {
                DisplayedItem::Marker(maker) => {
                    if maker.idx == idx {
                        Some(maker)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .is_none()
        {
            let maker = DisplayedMarker {
                color: None,
                background_color: None,
                name: None,
                idx,
            };
            self.displayed_items.push(DisplayedItem::Marker(maker));
        }
        self.markers.insert(idx, location.clone());
    }

    pub fn move_marker_to_cursor(&mut self, idx: u8) {
        let Some(location) = &self.cursor.clone() else {
            return;
        };
        self.set_marker_position(idx, &location)
    }

    pub fn draw_marker_number_boxes(
        &self,
        ctx: &mut DrawingContext,
        size: Vec2,
        theme: &SurferTheme,
        viewport: &Viewport,
    ) {
        let text_size = ctx.cfg.text_size;

        for displayed_item in self.displayed_items.iter().filter_map(|item| match item {
            DisplayedItem::Marker(maker) => Some(maker),
            _ => None,
        }) {
            let background_color = displayed_item
                .color
                .as_ref()
                .and_then(|color| theme.colors.get(color))
                .unwrap_or(&theme.cursor.color);

            let x = self.numbered_marker_location(displayed_item.idx, viewport, size.x);

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
    pub fn draw_marker_window(&self, waves: &WaveData, ctx: &Context, msgs: &mut Vec<Message>) {
        let mut open = true;

        let mut markers: Vec<(u8, &BigInt, WidgetText)> = vec![];
        if let Some(cursor) = &waves.cursor {
            markers.push((255, cursor, WidgetText::RichText(RichText::new("Primary"))))
        }

        let mut numbered_markers = waves
            .displayed_items
            .iter()
            .filter_map(|displayed_item| match displayed_item {
                DisplayedItem::Marker(marker) => {
                    let text_color = self.get_item_text_color(displayed_item);
                    Some((
                        marker.idx,
                        waves.numbered_marker_time(marker.idx),
                        marker.marker_text(text_color),
                    ))
                }
                _ => None,
            })
            .sorted_by(|a, b| Ord::cmp(&a.0, &b.0))
            .collect_vec();

        markers.append(&mut numbered_markers);
        Window::new("Markers")
            .collapsible(true)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    Grid::new("markers")
                        .striped(true)
                        .num_columns(markers.len() + 1)
                        .spacing([5., 5.])
                        .show(ui, |ui| {
                            ui.label("");
                            for (marker_idx, _, widget_text) in &markers {
                                if *marker_idx < 255 {
                                    ui.selectable_label(false, widget_text.clone())
                                        .clicked()
                                        .then(|| {
                                            msgs.push(Message::GoToMarkerPosition(*marker_idx, 0))
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
                            for (marker_idx, row_marker_time, row_widget_text) in &markers {
                                if *marker_idx < 255 {
                                    ui.selectable_label(false, row_widget_text.clone())
                                        .clicked()
                                        .then(|| {
                                            msgs.push(Message::GoToMarkerPosition(*marker_idx, 0))
                                        });
                                } else {
                                    ui.selectable_label(false, row_widget_text.clone())
                                        .clicked()
                                        .then(|| {
                                            msgs.push(Message::GoToTime(waves.cursor.clone(), 0))
                                        });
                                }
                                for (_, col_marker_time, _) in &markers {
                                    ui.label(time_string(
                                        &(*row_marker_time - *col_marker_time),
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

    pub fn draw_marker_boxes(
        &self,
        waves: &WaveData,
        ctx: &mut DrawingContext,
        view_width: f32,
        gap: f32,
        viewport: &Viewport,
    ) {
        let text_size = ctx.cfg.text_size;

        for drawing_info in waves.drawing_infos.iter().filter_map(|item| match item {
            ItemDrawingInfo::Marker(marker) => Some(marker),
            _ => None,
        }) {
            let Some(item) = waves.displayed_items.get(drawing_info.item_list_idx) else {
                return;
            };

            // We draw in absolute coords, but the variable offset in the y
            // direction is also in absolute coordinates, so we need to
            // compensate for that
            let y_zero = (ctx.to_screen)(0., 0.).y;
            let y_offset = drawing_info.top - y_zero;
            let y_bottom = drawing_info.bottom - y_zero;

            let background_color = item
                .color()
                .and_then(|color| self.config.theme.colors.get(&color))
                .unwrap_or(&self.config.theme.cursor.color);

            let x = waves.numbered_marker_location(drawing_info.idx, viewport, view_width);

            // Time string
            let time = time_string(
                waves
                    .markers
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
            let max = (ctx.to_screen)(rect.max.x, y_bottom + gap);
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
                self.config.theme.alt_text_color,
            );
        }
    }
}
