use std::time::Instant;

use log::warn;

pub struct TimedRegion {
    start: Option<Instant>,
    end: Option<Instant>,
}

impl TimedRegion {
    pub fn started() -> Self {
        Self {
            start: Some(Instant::now()),
            end: None
        }
    }

    pub fn defer() -> Self {
        Self {
            start: None,
            end: None
        }
    }

    pub fn start(&mut self) {
        self.start = Some(Instant::now())
    }

    pub fn stop(&mut self) {
        self.end = Some(Instant::now())
    }

    pub fn secs(self) -> f64 {
        let result = (self.end.unwrap() - self.start.unwrap()).as_secs_f64();
        std::mem::forget(self);
        result
    }
}

impl Drop for TimedRegion {
    fn drop(&mut self) {
        warn!("Dropping a timed region timer");
    }
}
