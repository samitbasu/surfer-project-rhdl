use std::time::Duration;

use num::{BigInt, BigRational};

#[derive(Debug, Clone)]
pub struct Viewport {
    pub curr_left: BigRational,
    pub curr_right: BigRational,
}

impl Viewport {
    pub fn new(left: BigInt, right: BigInt) -> Self {
        Self {
            curr_left: BigRational::from(left.clone()),
            curr_right: BigRational::from(right.clone()),
        }
    }

    pub fn interpolate(&mut self, _duration: Duration) {
    }
}

