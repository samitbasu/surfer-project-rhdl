use derive_more::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};
use num::{BigInt, BigRational, FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    Add,
    Sub,
    Mul,
    Neg,
    AddAssign,
    SubAssign,
    PartialOrd,
    PartialEq,
)]
pub struct Relative(pub f64);

impl Relative {
    pub fn absolute(&self, num_timestamps: &BigInt) -> Absolute {
        Absolute(
            self.0
                * num_timestamps
                    .to_f64()
                    .expect("Failed to convert timestamp to f64"),
        )
    }

    pub fn min(&self, other: &Relative) -> Self {
        Self(self.0.min(other.0))
    }
}

impl std::ops::Div for Relative {
    type Output = Relative;

    fn div(self, rhs: Self) -> Self::Output {
        Self(self.0 / rhs.0)
    }
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, Add, Sub, Mul, Neg, Div, PartialOrd, PartialEq,
)]
pub struct Absolute(pub f64);

impl Absolute {
    pub fn relative(&self, num_timestamps: &BigInt) -> Relative {
        Relative(
            self.0
                / num_timestamps
                    .to_f64()
                    .expect("Failed to convert timestamp to f64"),
        )
    }
}

impl std::ops::Div for Absolute {
    type Output = Absolute;

    fn div(self, rhs: Self) -> Self::Output {
        Self(self.0 / rhs.0)
    }
}

impl From<&BigInt> for Absolute {
    fn from(value: &BigInt) -> Self {
        Self(value.to_f64().expect("Failed to convert timestamp to f64"))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Viewport {
    pub curr_left: Relative,
    pub curr_right: Relative,
}

impl Viewport {
    pub fn new() -> Self {
        Self {
            curr_left: Relative(0.0),
            curr_right: Relative(1.0),
        }
    }

    pub fn to_time_f64(&self, x: f64, view_width: f32, num_timestamps: &BigInt) -> Absolute {
        let time_spacing = self.width_absolute(num_timestamps) / view_width as f64;

        self.curr_left.absolute(num_timestamps) + time_spacing * x
    }

    pub fn to_time_bigint(&self, x: f32, view_width: f32, num_timestamps: &BigInt) -> BigInt {
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self;

        let big_right = BigRational::from_f64(right.absolute(num_timestamps).0)
            .unwrap_or_else(|| BigRational::from_u8(1).unwrap());
        let big_left = BigRational::from_f64(left.absolute(num_timestamps).0)
            .unwrap_or_else(|| BigRational::from_u8(1).unwrap());
        let big_width =
            BigRational::from_f32(view_width).unwrap_or_else(|| BigRational::from_u8(1).unwrap());
        let big_x = BigRational::from_f32(x).unwrap_or_else(|| BigRational::from_u8(1).unwrap());

        let time = big_left.clone() + (big_right - big_left) / big_width * big_x;
        time.round().to_integer()
    }

    pub fn pixel_from_time(&self, time: &BigInt, view_width: f32, num_timestamps: &BigInt) -> f32 {
        let distance_from_left =
            Absolute(time.to_f64().unwrap()) - self.curr_left.absolute(num_timestamps);

        (((distance_from_left / self.width_absolute(num_timestamps)).0) * (view_width as f64))
            as f32
    }

    pub fn pixel_from_time_f64(
        &self,
        time: Absolute,
        view_width: f32,
        num_timestamps: &BigInt,
    ) -> f32 {
        let distance_from_left = time - self.curr_left.absolute(num_timestamps);

        (((distance_from_left / self.width_absolute(num_timestamps)).0) * (view_width as f64))
            as f32
    }

    pub fn clip_to(&self, old_num_timestamps: &BigInt, new_num_timestamps: &BigInt) -> Viewport {
        let resize_factor =
            (Absolute::from(new_num_timestamps) / Absolute::from(old_num_timestamps)).0;
        let curr_range = self.width();
        let valid_range = self.width() * resize_factor;

        // first fix the zoom if less than 10% of the screen are filled
        // do this first so that if the user had the waveform at a side
        // it stays there when moving, if centered it stays centered
        let fill_limit = Relative(0.1);
        let corr_zoom = fill_limit / (valid_range / curr_range);
        let zoom_fixed = if corr_zoom > Relative(1.0) {
            Viewport {
                curr_left: self.curr_left / corr_zoom,
                curr_right: self.curr_right / corr_zoom,
            }
        } else {
            *self
        };

        // scroll waveform less than 10% of the screen to the left & right
        // contain actual wave data, keep zoom as it was
        let overlap_limit = 0.1;
        let min_overlap = curr_range.min(&valid_range) * overlap_limit;
        let corr_right = ((self.curr_left * resize_factor) + min_overlap) - zoom_fixed.curr_right;
        let corr_left = ((self.curr_right * resize_factor) - min_overlap) - zoom_fixed.curr_left;
        if corr_right > Relative(0.0) {
            Viewport {
                curr_left: zoom_fixed.curr_left + corr_right,
                curr_right: zoom_fixed.curr_right + corr_right,
            }
        } else if corr_left < Relative(0.0) {
            Viewport {
                curr_left: zoom_fixed.curr_left + corr_left,
                curr_right: zoom_fixed.curr_right + corr_left,
            }
        } else {
            zoom_fixed
        }
    }

    #[inline]
    fn width(&self) -> Relative {
        self.curr_right - self.curr_left
    }

    #[inline]
    fn width_absolute(&self, num_timestamps: &BigInt) -> Absolute {
        self.width().absolute(num_timestamps)
    }

    pub fn go_to_time(&mut self, center: &BigInt, num_timestamps: &BigInt) {
        let center_point: Absolute = center.into();
        let half_width = self.half_width_absolute(num_timestamps);

        self.curr_left = (center_point - half_width).relative(num_timestamps);
        self.curr_right = (center_point + half_width).relative(num_timestamps);
    }

    pub fn zoom_to_fit(&mut self) {
        self.curr_left = Relative(0.0);
        self.curr_right = Relative(1.0);
    }

    pub fn go_to_start(&mut self) {
        let old_width = self.width();
        self.curr_left = Relative(0.0);
        self.curr_right = old_width;
    }

    pub fn go_to_end(&mut self) {
        self.curr_left = Relative(1.0) - self.width();
        self.curr_right = Relative(1.0);
    }

    pub fn handle_canvas_zoom(
        &mut self,
        mouse_ptr_timestamp: Option<BigInt>,
        delta: f64,
        num_timestamps: &BigInt,
    ) {
        // Zoom or scroll
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self;

        let (target_left, target_right) =
            match mouse_ptr_timestamp.map(|t| Absolute::from(&t).relative(num_timestamps)) {
                Some(mouse_location) => (
                    (*left - mouse_location) / Relative(delta) + mouse_location,
                    (*right - mouse_location) / Relative(delta) + mouse_location,
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
        let scroll_step = -self.width() / Relative(50. * 20.);
        let scaled_deltay = scroll_step * deltay;

        self.curr_left += scaled_deltay;
        self.curr_right += scaled_deltay;
    }

    #[inline]
    fn midpoint(&self) -> Relative {
        (self.curr_right + self.curr_left) * 0.5
    }

    #[inline]
    fn half_width(&self) -> Relative {
        self.width() * 0.5
    }

    #[inline]
    fn half_width_absolute(&self, num_timestamps: &BigInt) -> Absolute {
        (self.width() * 0.5).absolute(num_timestamps)
    }

    pub fn zoom_to_range(&mut self, left: &BigInt, right: &BigInt, num_timestamps: &BigInt) {
        self.curr_left = Absolute::from(left).relative(num_timestamps);
        self.curr_right = Absolute::from(right).relative(num_timestamps);
    }

    pub fn go_to_cursor_if_not_in_view(
        &mut self,
        cursor: &BigInt,
        num_timestamps: &BigInt,
    ) -> bool {
        let fcursor = cursor.into();
        if fcursor <= self.curr_left.absolute(num_timestamps)
            || fcursor >= self.curr_right.absolute(num_timestamps)
        {
            self.go_to_time_f64(fcursor, num_timestamps);
            true
        } else {
            false
        }
    }

    pub fn go_to_time_f64(&mut self, center: Absolute, num_timestamps: &BigInt) {
        let half_width = (self.curr_right.absolute(num_timestamps)
            - self.curr_left.absolute(num_timestamps))
            / 2.;

        self.curr_left = (center - half_width).relative(num_timestamps);
        self.curr_right = (center + half_width).relative(num_timestamps);
    }
}
