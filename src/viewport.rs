use std::time::Duration;

use num::{BigInt, BigRational};

#[derive(Debug, Clone)]
pub struct Viewport {
    pub curr_left: f64,
    pub curr_right: f64,
}

impl Viewport {
    pub fn new(left: f64, right: f64) -> Self {
        Self {
            curr_left: left,
            curr_right: right,
        }
    }

    pub fn interpolate(&mut self, _duration: Duration) {
    }
}

