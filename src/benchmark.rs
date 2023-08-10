use std::{collections::BTreeMap, time::Instant};

use itertools::Itertools;
use log::warn;

pub struct TimedRegion {
    start: Option<Instant>,
    end: Option<Instant>,
}

impl TimedRegion {
    pub fn started() -> Self {
        Self {
            start: Some(Instant::now()),
            end: None,
        }
    }

    pub fn defer() -> Self {
        Self {
            start: None,
            end: None,
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

    pub fn format(&self) -> String {
        self.timings
            .iter()
            .sorted_by_key(|(name, _)| name.as_str())
            .map(|(name, (counts, sub))| {
                let total: f64 = counts.iter().sum();
                let average = total / counts.len() as f64;

                let substr = sub
                    .iter()
                    .sorted_by_key(|(name, _)| name.as_str())
                    .map(|(name, counts)| {
                        let subtotal: f64 = counts.iter().sum();
                        let subaverage = total / counts.len() as f64;

                        let pct = (subtotal / total) * 100.;
                        format!("\t{name}: {subtotal:.05} {subaverage:.05} {pct:.05}%")
                    })
                    .join("\n");

                format!("{name}: {total:.05} ({average:.05})\n{substr}")
            })
            .join("\n")
    }
}
