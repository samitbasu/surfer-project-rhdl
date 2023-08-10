use num::{BigInt, BigRational, FromPrimitive, ToPrimitive};

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

    pub fn to_time(&self, x: f64, view_width: f32) -> BigRational {
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self;

        let time_spacing = (right - left) / view_width as f64;

        let time = left + time_spacing * x;
        BigRational::from_f64(time).unwrap_or_else(|| BigRational::from_f64(1.0f64).unwrap())
    }

    pub fn from_time(&self, time: &BigInt, view_width: f64) -> f64 {
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self;

        let time_float = time.to_f64().unwrap();

        let distance_from_left = time_float - left;

        let width = right - left;

        (distance_from_left / width) * view_width
    }
}
