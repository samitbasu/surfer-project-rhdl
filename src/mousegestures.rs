use eframe::egui::{self, Painter, RichText, Sense};
use eframe::emath::{Align2, RectTransform};
use eframe::epaint::{FontId, Pos2, Rect, Stroke, Vec2};
use num::ToPrimitive;

use crate::view::{time_string, DrawingContext};
use crate::{Message, State, VcdData};

#[derive(Clone, PartialEq, Copy)]
pub enum GestureKind {
    ZoomToFit,
    ZoomIn,
    ZoomOut,
    ScrollToEnd,
    ScrollToStart,
}

impl State {
    pub fn draw_mouse_gesture_widget(
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
                let distance = current_location - start_location;
                if distance.length_sq() >= self.config.gesture.deadzone {
                    match gesture_type(start_location, current_location) {
                        Some(GestureKind::ZoomToFit) => self.draw_gesture_line(
                            start_location,
                            current_location,
                            "Zoom to fit",
                            true,
                            ctx,
                        ),
                        Some(GestureKind::ZoomIn) => self.draw_zoom_in_gesture(
                            start_location,
                            current_location,
                            response,
                            ctx,
                            vcd,
                        ),

                        Some(GestureKind::ScrollToStart) => self.draw_gesture_line(
                            start_location,
                            current_location,
                            "Scroll to start",
                            true,
                            ctx,
                        ),
                        Some(GestureKind::ScrollToEnd) => self.draw_gesture_line(
                            start_location,
                            current_location,
                            "Scroll to end",
                            true,
                            ctx,
                        ),
                        Some(GestureKind::ZoomOut) => self.draw_gesture_line(
                            start_location,
                            current_location,
                            "Zoom out",
                            true,
                            ctx,
                        ),
                        _ => {
                            self.draw_gesture_line(start_location, current_location, "", false, ctx)
                        }
                    }
                } else {
                    self.draw_gesture_help(response, ctx.painter, Some(start_location));
                }
            });

            response
                .drag_released_by(egui::PointerButton::Middle)
                .then(|| {
                    let end_location = pointer_pos_canvas.unwrap();
                    let distance = end_location - start_location;
                    if distance.length_sq() >= self.config.gesture.deadzone {
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
                    }
                    msgs.push(Message::SetDragStart(None))
                });
        };
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
                self.config.gesture.style.color
            } else {
                self.config.gesture.style.color.gamma_multiply(0.3)
            },
            width: self.config.gesture.style.width,
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

    fn draw_zoom_in_gesture(
        &self,
        start_location: Pos2,
        current_location: Pos2,
        response: &egui::Response,
        ctx: &mut DrawingContext<'_>,
        vcd: &VcdData,
    ) {
        let stroke = Stroke {
            color: self.config.gesture.style.color,
            width: self.config.gesture.style.width,
        };
        let startx = start_location.x;
        let starty = start_location.y;
        let endx = current_location.x;
        let height = response.rect.size().y;
        let width = response.rect.size().x;
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
                        .to_time(minx as f64, width)
                        .round()
                        .to_integer()),
                    &vcd.inner.metadata,
                    &(self.wanted_timescale)
                ),
                time_string(
                    &(vcd
                        .viewport
                        .to_time(maxx as f64, width)
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

    pub fn mouse_gesture_help(&self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        egui::Window::new("Mouse gestures")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Press middle mouse button and drag"));
                    ui.add_space(20.);
                    let (response, painter) =
                        ui.allocate_painter(Vec2 { x: 300.0, y: 300.0 }, Sense::click());
                    self.draw_gesture_help(&response, &painter, None);
                    ui.add_space(10.);
                    ui.separator();
                    if ui.button("Close").clicked() {
                        msgs.push(Message::SetGestureHelpVisible(false))
                    }
                });
            });
        if !open {
            msgs.push(Message::SetGestureHelpVisible(false))
        }
    }

    fn draw_gesture_help(
        &self,
        response: &egui::Response,
        painter: &Painter,
        midpoint: Option<Pos2>,
    ) {
        // Compute sizes and coordinates
        let tan225 = 0.41421356237;
        let rect = response.rect;
        let width = rect.width();
        let height = rect.height();
        let (midx, midy, deltax, deltay) = if let Some(midpoint) = midpoint {
            (
                midpoint.x,
                midpoint.y,
                self.config.gesture.size / 2.0,
                self.config.gesture.size / 2.0,
            )
        } else {
            (width / 2.0, height / 2.0, width / 2.0, height / 2.0)
        };

        let container_rect = Rect::from_min_size(Pos2::ZERO, response.rect.size());
        let to_screen = &|x, y| {
            RectTransform::from_to(container_rect, rect)
                .transform_pos(Pos2::new(x, y) + Vec2::new(0.5, 0.5))
        };
        let stroke = Stroke {
            color: self.config.gesture.style.color,
            width: self.config.gesture.style.width,
        };
        let tan225deltax = tan225 * deltax;
        let tan225deltay = tan225 * deltay;
        // Draw lines
        painter.line_segment(
            [
                to_screen(midx - deltax, midy + tan225deltax),
                to_screen(midx + deltax, midy - tan225deltax),
            ],
            stroke,
        );
        painter.line_segment(
            [
                to_screen(midx - deltax, midy - tan225deltax),
                to_screen(midx + deltax, midy + tan225deltax),
            ],
            stroke,
        );
        painter.line_segment(
            [
                to_screen(midx + tan225deltay, midy - deltay),
                to_screen(midx - tan225deltay, midy + deltay),
            ],
            stroke,
        );
        painter.line_segment(
            [
                to_screen(midx - tan225deltay, midy - deltay),
                to_screen(midx + tan225deltay, midy + deltay),
            ],
            stroke,
        );

        let halfwaytexty_upper = (midy - deltay) + (deltay - tan225deltax) / 2.0;
        let halfwaytexty_lower = (midy + deltay) - (deltay - tan225deltax) / 2.0;
        // Draw commands
        painter.text(
            to_screen(midx - deltax, midy),
            Align2::LEFT_CENTER,
            "Zoom in",
            FontId::default(),
            self.config.theme.foreground,
        );
        painter.text(
            to_screen(midx + deltax, midy),
            Align2::RIGHT_CENTER,
            "Zoom in",
            FontId::default(),
            self.config.theme.foreground,
        );
        painter.text(
            to_screen(midx - deltax, halfwaytexty_upper),
            Align2::LEFT_CENTER,
            "Zoom to fit",
            FontId::default(),
            self.config.theme.foreground,
        );
        painter.text(
            to_screen(midx + deltax, halfwaytexty_upper),
            Align2::RIGHT_CENTER,
            "Zoom out",
            FontId::default(),
            self.config.theme.foreground,
        );
        painter.text(
            to_screen(midx, midy - deltay),
            Align2::CENTER_TOP,
            "Cancel",
            FontId::default(),
            self.config.theme.foreground,
        );
        painter.text(
            to_screen(midx - deltax, halfwaytexty_lower),
            Align2::LEFT_CENTER,
            "Go to start",
            FontId::default(),
            self.config.theme.foreground,
        );
        painter.text(
            to_screen(midx + deltax, halfwaytexty_lower),
            Align2::RIGHT_CENTER,
            "Go to end",
            FontId::default(),
            self.config.theme.foreground,
        );
        painter.text(
            to_screen(midx, midy + deltay),
            Align2::CENTER_BOTTOM,
            "Cancel",
            FontId::default(),
            self.config.theme.foreground,
        );
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
