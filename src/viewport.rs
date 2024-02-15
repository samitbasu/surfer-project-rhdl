use num::{BigInt, BigRational, FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

    pub fn to_time_f64(&self, x: f64, view_width: f32) -> f64 {
        let time_spacing = self.width() / view_width as f64;

        self.curr_left + time_spacing * x
    }

    pub fn to_time_bigint(&self, x: f32, view_width: f32) -> BigInt {
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self;

        let big_right =
            BigRational::from_f64(*right).unwrap_or_else(|| BigRational::from_u8(1).unwrap());
        let big_left =
            BigRational::from_f64(*left).unwrap_or_else(|| BigRational::from_u8(1).unwrap());
        let big_width =
            BigRational::from_f32(view_width).unwrap_or_else(|| BigRational::from_u8(1).unwrap());
        let big_x = BigRational::from_f32(x).unwrap_or_else(|| BigRational::from_u8(1).unwrap());

        let time = big_left.clone() + (big_right - big_left) / big_width * big_x;
        time.round().to_integer()
    }

    pub fn from_time(&self, time: &BigInt, view_width: f32) -> f32 {
        let distance_from_left = time.to_f64().unwrap() - self.curr_left;

        ((distance_from_left / self.width()) * view_width as f64) as f32
    }

    pub fn from_time_f64(&self, time: f64, view_width: f32) -> f32 {
        let distance_from_left = time - self.curr_left;

        ((distance_from_left / self.width()) * view_width as f64) as f32
    }

    pub fn clip_to(&self, valid: &Viewport) -> Viewport {
        let curr_range = self.width();
        let valid_range = valid.width();

        // first fix the zoom if less than 10% of the screen are filled
        // do this first so that if the user had the waveform at a side
        // it stays there when moving, if centered it stays centered
        let fill_limit = 0.1;
        let corr_zoom = fill_limit / (valid_range / curr_range);
        let zoom_fixed = if corr_zoom > 1.0 {
            Viewport::new(self.curr_left / corr_zoom, self.curr_right / corr_zoom)
        } else {
            *self
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

    #[inline]
    fn width(&self) -> f64 {
        self.curr_right - self.curr_left
    }

    pub fn go_to_time(&mut self, center: &BigInt) {
        let center_point = center.to_f64().unwrap();
        let half_width = self.half_width();

        self.curr_left = center_point - half_width;
        self.curr_right = center_point + half_width;
    }

    pub fn zoom_to_fit(&mut self, num_timestamps: &BigInt) {
        self.curr_left = 0.0;
        self.curr_right = num_timestamps.to_f64().unwrap();
    }

    pub fn go_to_start(&mut self) {
        let old_width = self.width();
        self.curr_left = 0.0;
        self.curr_right = old_width;
    }

    pub fn go_to_end(&mut self, num_timestamps: &BigInt) {
        let end_point = num_timestamps.to_f64().unwrap();
        self.curr_left = end_point - self.width();
        self.curr_right = end_point;
    }

    pub fn handle_canvas_zoom(&mut self, mouse_ptr_timestamp: Option<f64>, delta: f64) {
        // Zoom or scroll
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self;

        let (target_left, target_right) = match mouse_ptr_timestamp {
            Some(mouse_location) => (
                (left - mouse_location) / delta + mouse_location,
                (right - mouse_location) / delta + mouse_location,
            ),
            None => {
                let mid_point = self.midpoint();
                let offset = self.half_width() * delta;

                (mid_point - offset, mid_point + offset)
            }
        };

        self.curr_left = target_left;
        self.curr_right = target_right;
    }

    pub fn handle_canvas_scroll(&mut self, deltay: f64) {
        // Scroll 5% of the viewport per scroll event.
        // One scroll event yields 50
        let scroll_step = -self.width() / (50. * 20.);
        let scaled_deltay = scroll_step * deltay;

        self.curr_left += scaled_deltay;
        self.curr_right += scaled_deltay;
    }

    #[inline]
    fn midpoint(&self) -> f64 {
        (self.curr_right + self.curr_left) * 0.5
    }

    #[inline]
    fn half_width(&self) -> f64 {
        self.width() * 0.5
    }

    pub fn zoom_to_range(&mut self, left: f64, right: f64) {
        self.curr_left = left;
        self.curr_right = right;
    }

    pub fn go_to_cursor_if_not_in_view(&mut self, cursor: &BigInt) -> bool {
        let fcursor = cursor.to_f64().unwrap();
        if fcursor <= self.curr_left || fcursor >= self.curr_right {
            self.go_to_time_f64(fcursor);
            true
        } else {
            false
        }
    }

    pub fn go_to_time_f64(&mut self, center: f64) {
        let half_width = (self.curr_right - self.curr_left) / 2.;

        self.curr_left = center - half_width;
        self.curr_right = center + half_width;
    }
}
