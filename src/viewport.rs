use std::time::Duration;

use num::{BigInt, BigRational, FromPrimitive};

#[derive(Debug, Clone)]
pub struct Viewport {
    curr_left: BigRational,
    curr_right: BigRational,

    // Computing the timestamp value of a pixel on the canvas is expensive and needs to be done
    // once per pixel. The initial part of that computation is computing the width
    // of a single pixel. We'll cache that value here for performance reasons
    /// The width of the viewport in pixels
    drawn_width: f32,
    /// The width of a pixel in timestamps
    pixel_width: BigRational,
}

impl Viewport {
    pub fn new(left: BigInt, right: BigInt) -> Self {
        Self {
            curr_left: BigRational::from(left.clone()),
            curr_right: BigRational::from(right.clone()),
            drawn_width: 1.,
            pixel_width: BigRational::from_float(1.).unwrap()
        }
    }

    pub fn left(&self) -> &BigRational {
        &self.curr_left
    }

    pub fn right(&self) -> &BigRational {
        &self.curr_right
    }

    pub fn set_bounds(&mut self, left: BigRational, right: BigRational) {
        self.curr_left = left;
        self.curr_right = right;
        self.recompute_pixel_width()
    }

    pub fn set_drawn_width(&mut self, new: f32) {
        if new != self.drawn_width {
            self.drawn_width = new;
            self.recompute_pixel_width()
        }
    }

    pub fn recompute_pixel_width(&mut self) {
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self;

        if self.drawn_width != 0. {
            self.pixel_width = (right - left) / BigRational::from_float(self.drawn_width).unwrap();
        }
    }

    pub fn pixel_to_timestamp(&self, x: BigRational) -> BigRational {
        println!("{} {} {x}", self.curr_left, self.pixel_width);
        let time = &self.curr_left + &self.pixel_width * x;
        time
    }
}

