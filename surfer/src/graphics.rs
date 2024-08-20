use eframe::{
    emath::{Align, Align2},
    epaint::{Color32, CubicBezierShape, FontId, Shape, Stroke, Vec2},
};
use num::BigInt;
use serde::{Deserialize, Serialize};

use crate::{
    displayed_item::DisplayedItemRef, view::DrawingContext, viewport::Viewport, wave_data::WaveData,
};

#[derive(Serialize, Deserialize, Debug)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub fn as_vector(&self) -> Vec2 {
        match self {
            Direction::North => Vec2::new(0., -1.),
            Direction::East => Vec2::new(1., 0.),
            Direction::South => Vec2::new(0., 1.),
            Direction::West => Vec2::new(-1., 0.),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Anchor {
    Top,
    Center,
    Bottom,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GraphicsY {
    pub item: DisplayedItemRef,
    pub anchor: Anchor,
}

/// A point used to place graphics.
#[derive(Serialize, Deserialize, Debug)]
pub struct GrPoint {
    /// Timestamp at which to place the graphic
    pub x: BigInt,
    pub y: GraphicsY,
}

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash, Debug)]
pub struct GraphicId(pub usize);

#[derive(Serialize, Deserialize, Debug)]
pub enum Graphic {
    TextArrow {
        from: (GrPoint, Direction),
        to: (GrPoint, Direction),
        text: String,
    },
}

impl WaveData {
    // FIXME: This function should probably not be here, we should instead update ItemDrawingInfo to
    // have this info
    fn get_item_y(&self, y: &GraphicsY) -> Option<f32> {
        self.displayed_items_order
            .iter()
            .zip(&self.drawing_infos)
            .find(|(item, _info)| **item == y.item)
            .map(|(_, info)| match y.anchor {
                Anchor::Top => info.top(),
                Anchor::Center => info.top() + (info.bottom() - info.top()) / 2.,
                Anchor::Bottom => info.bottom(),
            })
            .map(|point| point - self.top_item_draw_offset)
    }

    pub(crate) fn draw_graphics(&self, ctx: &mut DrawingContext, size: Vec2, viewport: &Viewport) {
        for (_, g) in &self.graphics {
            match g {
                Graphic::TextArrow {
                    from: (from_point, from_dir),
                    to: (to_point, to_dir),
                    text,
                } => {
                    let from_x =
                        viewport.pixel_from_time(&from_point.x, size.x, &self.num_timestamps());
                    let from_y = self.get_item_y(&from_point.y);

                    let to_x =
                        viewport.pixel_from_time(&to_point.x, size.x, &self.num_timestamps());
                    let to_y = self.get_item_y(&to_point.y);

                    if let (Some(from_y), Some(to_y)) = (from_y, to_y) {
                        let from_dir = from_dir.as_vector() * 30.;
                        let to_dir = to_dir.as_vector() * 30.;
                        let shape = Shape::CubicBezier(CubicBezierShape {
                            points: [
                                (ctx.to_screen)(from_x, from_y),
                                (ctx.to_screen)(from_x + from_dir.x, from_y + from_dir.y),
                                (ctx.to_screen)(to_x + to_dir.x, to_y + to_dir.y),
                                (ctx.to_screen)(to_x, to_y),
                            ],
                            closed: false,
                            fill: Color32::TRANSPARENT,
                            stroke: Stroke {
                                width: 3.,
                                color: Color32::YELLOW,
                            }
                            .into(),
                        });
                        ctx.painter.add(shape);

                        ctx.painter.text(
                            (ctx.to_screen)(to_x, to_y),
                            Align2([Align::LEFT, Align::Center]),
                            text,
                            FontId::monospace(15.),
                            Color32::YELLOW,
                        );
                    }
                }
            }
        }
    }
}