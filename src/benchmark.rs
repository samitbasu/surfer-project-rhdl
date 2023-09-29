use std::{collections::BTreeMap, time::Instant};

use log::warn;

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub struct TimedRegion {
    start: Option<Instant>,
    end: Option<Instant>,
}

#[cfg(not(target_arch = "wasm32"))]
impl TimedRegion {
    pub fn started() -> Self {
        Self {
            start: Some(Instant::now()),
            end: None,
        }
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

#[cfg(target_arch = "wasm32")]
impl TimedRegion {
    pub fn started() -> Self {
        Self {
            start: None,
            end: None,
        }
    }

    pub fn stop(&mut self) {
        //self.end = Some(Instant::now())
    }

    pub fn secs(self) -> f64 {
        // let result = (self.end.unwrap() - self.start.unwrap()).as_secs_f64();
        // std::mem::forget(self);
        // result
        0.
    }
}

impl Drop for TimedRegion {
    fn drop(&mut self) {
        warn!("Dropping a timed region timer");
    }
}

pub struct TranslationTimings {
    timings: BTreeMap<String, (Vec<f64>, BTreeMap<String, Vec<f64>>)>,
}

impl TranslationTimings {
    pub fn new() -> Self {
        Self {
            timings: BTreeMap::new(),
        }
    }

    pub fn push_timing(&mut self, name: &str, subname: Option<&str>, timing: f64) {
        let target = self.timings.entry(name.to_string()).or_default();

        if let Some(subname) = subname {
            target
                .1
                .entry(subname.to_string())
                .or_default()
                .push(timing)
        }
        if subname.is_none() {
            target.0.push(timing)
        }
    }
}
