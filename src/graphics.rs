use num::BigInt;
use serde::{Deserialize, Serialize};

use crate::displayed_item::DisplayedItemRef;

#[derive(Serialize, Deserialize)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

#[derive(Serialize, Deserialize)]
pub enum Anchor {
    Top,
    Center,
    Bottom,
}

#[derive(Serialize, Deserialize)]
pub struct GraphicsY {
    item: DisplayedItemRef,
    anchor: Anchor,
}

/// A point used to place graphics.
#[derive(Serialize, Deserialize)]
pub struct GPoint {
    /// Timestamp at which to place the graphic
    x: BigInt,
    y: GraphicsY,
}

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct GraphicId(usize);

#[derive(Serialize, Deserialize)]
pub enum Graphic {
    TextArrow {
        from: (GPoint, Direction),
        to: (GPoint, Direction),
        text: String,
    },
}
