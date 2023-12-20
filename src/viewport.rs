use num::{BigInt, BigRational, FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn to_f64_time(&self, x: f64, view_width: f32) -> f64 {
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self;

        let time_spacing = (right - left) / view_width as f64;

        let time = left + time_spacing * x;
        time
    }

    pub fn to_bigint_time(&self, x: f64, view_width: f32) -> BigInt {
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self;

        let big_right =
            BigRational::from_f64(*right).unwrap_or_else(|| BigRational::from_f64(1.0f64).unwrap());
        let big_left =
            BigRational::from_f64(*left).unwrap_or_else(|| BigRational::from_f64(1.0f64).unwrap());
        let big_width = BigRational::from_f64(view_width as f64)
            .unwrap_or_else(|| BigRational::from_f64(1.0f64).unwrap());
        let big_x =
            BigRational::from_f64(x).unwrap_or_else(|| BigRational::from_f64(1.0f64).unwrap());

        let time = big_left.clone() + (big_right - big_left) / big_width * big_x;
        time.round().to_integer()
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

    pub fn clip_to(&self, valid: &Viewport) -> Viewport {
        let curr_range = self.curr_right - self.curr_left;
        let valid_range = valid.curr_right - valid.curr_left;

        // first fix the zoom if less than 10% of the screen are filled
        // do this first so that if the user had the waveform at a side
        // it stays there when moving, if centered it stays centered
        let fill_limit = 0.1;
        let corr_zoom = fill_limit / (valid_range / curr_range);
        let zoom_fixed = if corr_zoom > 1.0 {
            Viewport::new(self.curr_left / corr_zoom, self.curr_right / corr_zoom)
        } else {
            self.clone()
        };

        // scroll waveform less than 10% of the screen to the left & right
        // contain actual wave data, keep zoom as it was
        let overlap_limit = 0.1;
        let min_overlap = curr_range.min(valid_range) * overlap_limit;
        let corr_right = (valid.curr_left + min_overlap) - zoom_fixed.curr_right;
        let corr_left = (valid.curr_right - min_overlap) - zoom_fixed.curr_left;
        if corr_right > 0.0 {
            Viewport::new(
                zoom_fixed.curr_left + corr_right,
                zoom_fixed.curr_right + corr_right,
            )
        } else if corr_left < 0.0 {
            Viewport::new(
                zoom_fixed.curr_left + corr_left,
                zoom_fixed.curr_right + corr_left,
            )
        } else {
            zoom_fixed
        }
    }
}
