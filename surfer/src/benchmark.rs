//! Drawing and handling of the performance plot.
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap, VecDeque},
    time::{Duration, Instant},
};

use egui_plot::{Legend, Line, Plot, PlotPoints, PlotUi};
use itertools::Itertools;
use log::warn;

use crate::{message::Message, State};

pub const NUM_PERF_SAMPLES: usize = 1000;

struct TimingRegion {
    durations: VecDeque<Duration>,
    start: Option<Instant>,
    end: Option<Instant>,
    subregions: BTreeSet<String>,
}

impl TimingRegion {
    pub fn start(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start = Some(Instant::now());
        }
    }
    pub fn stop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.end = Some(Instant::now());
        }
    }
}

pub struct Timing {
    active_region: Vec<String>,
    regions: HashMap<Vec<String>, TimingRegion>,
}

impl Timing {
    pub fn new() -> Self {
        let initial = vec![(
            vec![],
            TimingRegion {
                durations: VecDeque::new(),
                start: None,
                end: None,
                subregions: BTreeSet::new(),
            },
        )]
        .into_iter()
        .collect();

        Self {
            active_region: vec![],
            regions: initial,
        }
    }

    pub fn start_frame(&mut self) {
        if !self.active_region.is_empty() {
            warn!(
                "Starting frame with active region {}",
                self.active_region.join(".")
            );
        }
        for reg in self.regions.values_mut() {
            reg.start = None;
            reg.end = None;
        }
        if let Some(r) = self.regions.get_mut(&vec![]) {
            r.start();
        }
    }

    pub fn end_frame(&mut self) {
        if let Some(r) = self.regions.get_mut(&vec![]) {
            r.stop();
        }

        if !self.active_region.is_empty() {
            warn!(
                "Ended frame with active region {}",
                self.active_region.join(".")
            );
        }

        for (path, reg) in &mut self.regions {
            match (reg.start, reg.end) {
                (Some(start), Some(end)) => reg.durations.push_back(end - start),
                (None, Some(_)) => {
                    warn!(
                        "Timing region [{}] was stopped but not started",
                        path.join(".")
                    );
                    reg.durations.push_back(Duration::ZERO);
                }
                (Some(_), None) => {
                    warn!(
                        "Timing region [{}] was satrted but not stopped",
                        path.join(".")
                    );
                    reg.durations.push_back(Duration::ZERO);
                }
                (None, None) => reg.durations.push_back(Duration::ZERO),
            }
            if reg.durations.len() > NUM_PERF_SAMPLES {
                reg.durations.pop_front();
            }
            reg.start = None;
            reg.end = None;
        }
    }

    pub fn start(&mut self, name: impl Into<String>) {
        let name = name.into();
        if let Some(reg) = self.regions.get_mut(&self.active_region) {
            if !reg.subregions.contains(&name) {
                reg.subregions.insert(name.clone());
            }
        }

        self.active_region.push(name);

        let region = self
            .regions
            .entry(self.active_region.clone())
            .or_insert_with(|| TimingRegion {
                durations: VecDeque::new(),
                start: None,
                end: None,
                subregions: BTreeSet::new(),
            });
        region.start();
    }

    pub fn end(&mut self, name: impl Into<String>) {
        let name = name.into();
        if let Some(reg) = self.regions.get_mut(&self.active_region) {
            reg.stop();
        } else {
            warn!(
                "did not find a timing region {}",
                self.active_region.join(".")
            );
        }
        if let Some(reg_name) = self.active_region.pop() {
            if reg_name != name {
                warn!("Ended timing region {reg_name} but used {name}. Timing reports will be unreliable");
            }
        } else {
            warn!("Ended timing region {name} with no timing region active");
        }
    }
}

impl State {
    pub fn draw_performance_graph(&self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        egui::Window::new("Frame times")
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .default_width(700.)
            .show(ctx, |ui| {
                let timing = self.sys.timing.borrow_mut();

                let frame_times_f32 = timing.regions[&vec![]]
                    .durations
                    .iter()
                    .map(|t| t.as_nanos() as f32 / 1_000_000_000.)
                    .collect::<Vec<_>>();

                ui.horizontal(|ui| {
                    let mut redraw_state = self.sys.continuous_redraw;
                    ui.checkbox(&mut redraw_state, "Continuous redraw");
                    if redraw_state != self.sys.continuous_redraw {
                        msgs.push(Message::SetContinuousRedraw(redraw_state));
                    }

                    let f32_cmp = |a: &f32, b: &f32| a.partial_cmp(b).unwrap_or(Ordering::Equal);
                    ui.horizontal(|ui| {
                        ui.monospace(format!(
                            "99%: {:.3}",
                            frame_times_f32.iter().cloned().sum::<f32>()
                                / frame_times_f32.len() as f32
                        ));
                        ui.monospace(format!(
                            "Average: {:.3}",
                            frame_times_f32
                                .iter()
                                .cloned()
                                .sorted_by(f32_cmp)
                                .skip((frame_times_f32.len() as f32 * 0.99) as usize)
                                .sum::<f32>()
                                / (frame_times_f32.len() as f32 * 0.99)
                        ));

                        ui.monospace(format!(
                            "min: {:.3}",
                            frame_times_f32
                                .iter()
                                .cloned()
                                .min_by(f32_cmp)
                                .unwrap_or(0.)
                        ));

                        ui.monospace(format!(
                            "max: {:.3}",
                            frame_times_f32
                                .iter()
                                .cloned()
                                .max_by(f32_cmp)
                                .unwrap_or(0.)
                        ));
                    })
                });

                let plot = Plot::new("frame time")
                    .legend(Legend::default())
                    .show_axes([true, true])
                    .show_grid([true, true])
                    .include_x(0)
                    .include_x(NUM_PERF_SAMPLES as f64);

                plot.show(ui, |plot_ui| {
                    plot_ui.line(
                        Line::new(PlotPoints::from_ys_f32(
                            &self
                                .sys
                                .rendering_cpu_times
                                .iter()
                                .cloned()
                                .collect::<Vec<_>>(),
                        ))
                        .name("egui CPU draw time"),
                    );

                    draw_timing_region(plot_ui, &vec![], &timing);

                    plot_ui.line(
                        Line::new(PlotPoints::new(vec![
                            [0., 1. / 60.],
                            [NUM_PERF_SAMPLES as f64, 1. / 60.],
                        ]))
                        .name("60 fps"),
                    );
                    plot_ui.line(
                        Line::new(PlotPoints::new(vec![
                            [0., 1. / 30.],
                            [NUM_PERF_SAMPLES as f64, 1. / 30.],
                        ]))
                        .name("30 fps"),
                    );
                });
            });
        if !open {
            msgs.push(Message::SetPerformanceVisible(false));
        }
    }
}

pub fn draw_timing_region(plot_ui: &mut PlotUi, region: &Vec<String>, timing: &Timing) {
    let reg = &timing.regions[region];

    for sub in &reg.subregions {
        let mut new_region = region.clone();
        new_region.push(sub.clone());
        draw_timing_region(plot_ui, &new_region, timing);
    }
    let times_f32 = timing.regions[region]
        .durations
        .iter()
        .map(|t| t.as_nanos() as f32 / 1_000_000_000.)
        .collect::<Vec<_>>();

    plot_ui.line(
        Line::new(PlotPoints::from_ys_f32(&times_f32)).name(if region.is_empty() {
            "total".to_string()
        } else {
            region.join(".")
        }),
    );
}
