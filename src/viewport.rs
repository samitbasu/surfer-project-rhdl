use std::time::Duration;

use num::{BigInt, BigRational, FromPrimitive, Signed};

#[derive(Debug, Clone)]
pub struct Viewport {
    pub curr_left: BigRational,
    pub curr_right: BigRational,
    pub target_left: BigRational,
    pub target_right: BigRational,
    pub step_left: BigRational,
    pub step_right: BigRational,
}

impl Viewport {
    pub fn new(left: BigInt, right: BigInt) -> Self {
        Self {
            curr_left: BigRational::from(left.clone()),
            curr_right: BigRational::from(right.clone()),
            target_left: BigRational::from(left),
            target_right: BigRational::from(right),
            step_left: BigRational::from_float(0.).unwrap(),
            step_right: BigRational::from_float(0.).unwrap(),
        }
    }

    pub fn interpolate(&mut self, duration: Duration) {
        let duration = BigRational::from_integer(BigInt::from_u128(duration.as_millis()).unwrap())
            / BigRational::from_float(1000.).unwrap();
        self.curr_left = interp(
            &self.curr_left,
            &self.target_left,
            &self.step_left,
            &duration,
        );
        self.curr_right = interp(
            &self.curr_right,
            &self.target_right,
            &self.step_right,
            &duration,
        );
    }
}

pub fn interp(
    _curr: &BigRational,
    target: &BigRational,
    _step: &BigRational,
    _duration: &BigRational,
) -> BigRational {
    target.clone()
    // // Aim for accomplishing the move in 0.1 seconds
    // let distance_per_sec = step * BigRational::from_float(20.).unwrap();

    // let max_step = distance_per_sec * duration;

    // let distance_to_move = (target - curr)
    //     .max(-&max_step)
    //     .min(max_step);

    // println!("{curr} => {target} move: {distance_to_move}");

    // if (target - curr).abs() <= distance_to_move {
    //     target.clone()
    // }
    // else {
    //     curr + distance_to_move
    // }
}
