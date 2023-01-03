// mod signal_canvas;
mod translation;
// mod view;
mod viewport;

use eframe::egui;
use eframe::epaint;
use fastwave_backend::parse_vcd;
use fastwave_backend::ScopeIdx;
use fastwave_backend::SignalIdx;

use fastwave_backend::VCD;
use num::bigint::ToBigInt;
use num::BigInt;
use num::FromPrimitive;
use translation::TranslatorList;
use viewport::Viewport;

use std::collections::HashMap;
use std::fs::File;
use std::time::Instant;

use crate::translation::pytranslator::PyTranslator;

enum Command {
    None,
    Loopback(Vec<Message>),
}

fn main() {
    let state = State::new();

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(320.0, 240.0)),
        ..Default::default()
    };
    eframe::run_native("My egui App", options, Box::new(|_cc| Box::new(state)))
}

struct State {
    vcd: Option<VCD>,
    active_scope: Option<ScopeIdx>,
    signals: Vec<SignalIdx>,
    /// The offset of the left side of the wave window in signal timestamps.
    viewport: Viewport,
    control_key: bool,
    last_tick: Instant,
    num_timestamps: BigInt,
    /// Which translator to use for each signal
    signal_format: HashMap<SignalIdx, String>,
    translators: TranslatorList,
}

#[derive(Debug, Clone)]
enum Message {
    HierarchyClick(ScopeIdx),
    VarsScrolled(f32),
    AddSignal(SignalIdx),
    ControlKeyChange(bool),
    ChangeViewport(Viewport),
    Tick(Instant),
    SignalFormatChange(SignalIdx, String),
}

impl State {
    fn new() -> State {
        println!("Loading vcd");
        let file = File::open("cpu.vcd").expect("failed to open vcd");
        println!("Done loading vcd");

        let vcd = Some(parse_vcd(file).expect("Failed to parse vcd"));
        let num_timestamps = vcd
            .as_ref()
            .and_then(|vcd| vcd.max_timestamp().as_ref().map(|t| t.to_bigint().unwrap()))
            .unwrap_or(BigInt::from_u32(1).unwrap());

        let translators = TranslatorList::new(vec![
            Box::new(translation::HexTranslator {}),
            Box::new(translation::UnsignedTranslator {}),
            Box::new(PyTranslator::new("pytest", "translation_test.py").unwrap()),
        ]);

        State {
            active_scope: None,
            signals: vec![],
            control_key: false,
            viewport: Viewport::new(BigInt::from_u32(0).unwrap(), num_timestamps.clone()),
            last_tick: Instant::now(),
            num_timestamps,
            vcd,
            signal_format: HashMap::new(),
            translators,
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::HierarchyClick(scope) => self.active_scope = Some(scope),
            Message::VarsScrolled(_) => {}
            Message::AddSignal(s) => self.signals.push(s),
            Message::ControlKeyChange(val) => self.control_key = val,
            Message::ChangeViewport(new) => self.viewport = new,
            Message::Tick(instant) => {
                self.viewport.interpolate(instant - self.last_tick);
                self.last_tick = instant;
            }
            Message::SignalFormatChange(idx, format) => {
                if self.translators.inner.contains_key(&format) {
                    *self.signal_format.entry(idx).or_default() = format
                }
                else {
                    println!("WARN: No translator {format}")
                }
            }
        }
    }
}

impl eframe::App for State {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let max_width = 400.0;

        let mut msgs = vec![];

        egui::SidePanel::left("signal select left panel")
            .default_width(300.)
            .width_range(100.0..=max_width)
            .show(ctx, |ui| {
                ui.with_layout(
                    egui::Layout::top_down(egui::Align::Center).with_cross_justify(true),
                    |ui| {
                        ui.heading("Modules");
                        ui.add_space(3.0);
                    },
                );
                egui::Frame::none().show(ui, |ui| {
                    ui.with_layout(
                        egui::Layout::top_down(egui::Align::LEFT).with_cross_justify(true),
                        |ui| {
                            egui::ScrollArea::both().show(ui, |ui| {
                                ui.style_mut().wrap = Some(false);
                                if let Some(vcd) = &self.vcd {
                                    self.draw_all_scopes(&mut msgs, vcd, ui);
                                }
                            });
                        },
                    );
                })
            });

        for msg in msgs {
            self.update(msg);
        }
    }
}

impl State {
    fn draw_all_scopes(&self, msgs: &mut Vec<Message>, vcd: &VCD, ui: &mut egui::Ui) {
        for idx in vcd.root_scopes_by_idx() {
            self.draw_selectable_child_or_orphan_scope(msgs, vcd, idx, ui);
        }
    }

    fn draw_selectable_child_or_orphan_scope(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VCD,
        scope_idx: fastwave_backend::ScopeIdx,
        ui: &mut egui::Ui,
    ) {
        let name = vcd.scope_name_by_idx(scope_idx);
        let fastwave_backend::ScopeIdx(idx) = scope_idx;
        if vcd.child_scopes_by_idx(scope_idx).is_empty() {
            ui.add(egui::SelectableLabel::new(
                self.active_scope == Some(scope_idx),
                name,
            ))
            .clicked()
            .then(|| msgs.push(Message::HierarchyClick(scope_idx)));
        } else {
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                egui::Id::new(idx),
                false,
            )
            .show_header(ui, |ui| {
                ui.with_layout(
                    egui::Layout::top_down(egui::Align::LEFT).with_cross_justify(true),
                    |ui| {
                        ui.add(egui::SelectableLabel::new(
                            self.active_scope == Some(scope_idx),
                            name,
                        ))
                        .clicked()
                        .then(|| msgs.push(Message::HierarchyClick(scope_idx)))
                    },
                );
            })
            .body(|ui| self.draw_root_scope_view(msgs, vcd, scope_idx, ui));
        }
    }

    fn draw_root_scope_view(
        &self,
        msgs: &mut Vec<Message>,
        vcd: &VCD,
        root_idx: fastwave_backend::ScopeIdx,
        ui: &mut egui::Ui,
    ) {
        for child_scope_idx in vcd.child_scopes_by_idx(root_idx) {
            self.draw_selectable_child_or_orphan_scope(msgs, vcd, child_scope_idx, ui);
        }
    }
}

// fn expand_scopes<'a, 'b>(ctx: &egui::Context, vcd: &'a VCD, scopes: &'a [ScopeIdx]) {
//     let elems = scopes
//         .iter()
//         .map(|s| {
//             let name = vcd.scope_name_by_idx(s.clone());
//
//             let self_elem = button(text(name)).on_press(Message::HierarchyClick(s.clone()));
//             let children = expand_scopes(vcd, &vcd.child_scopes_by_idx(s.clone()));
//
//             let child_container = container(
//                 row!(horizontal_space(10.into()), children).align_items(Alignment::Start),
//             );
//
//             container(
//                 column![self_elem, child_container]
//                     .align_items(Alignment::Start)
//                     .spacing(1),
//             )
//         })
//         .collect::<Vec<_>>();
//
//     container(Column::with_children(
//         elems.into_iter().map(|c| c.into()).collect(),
//     ))
//     .into()
// }

// pub struct SignalSelect {
//     vcd: Rc<fastwave_backend::VCD>,
//     selected_module: fastwave_backend::ScopeIdx,
// }
//
// impl SignalSelect {
//     pub fn new() -> Self {
//         SignalSelect {
//             vcd: vcd,
//             selected_module: fastwave_backend::ScopeIdx(0),
//         }
//     }
//     pub fn draw(&mut self, ctx: &egui::Context, theme_manager: &theme::ThemeManager) {
//         let max_width = 400.0;
//         let window_round = epaint::Rounding::same(20.);
//
//         let fill = if false {
//             egui::Color32::from_rgba_premultiplied(0, 0, 0, 100)
//         } else {
//             egui::Color32::from_rgba_premultiplied(0, 0, 0, 25)
//         };
//         egui::SidePanel::left("signal select left panel")
//             .default_width(300.)
//             .width_range(100.0..=max_width)
//             .show(ctx, |ui| {
//                 ui.with_layout(
//                     egui::Layout::top_down(egui::Align::Center).with_cross_justify(true),
//                     |ui| {
//                         ui.heading("Modules");
//                         ui.add_space(3.0);
//                     },
//                 );
//                 theme_manager.new_frame().show(ui, |ui| {
//                     ui.with_layout(
//                         egui::Layout::top_down(egui::Align::LEFT).with_cross_justify(true),
//                         |ui| {
//                             egui::ScrollArea::both().show(ui, |ui| {
//                                 ui.style_mut().wrap = Some(false);
//                                 self.draw_all_scopes(ui);
//                             });
//                         },
//                     );
//                 });
//             });
//     }
//     fn draw_all_scopes(&mut self, ui: &mut egui::Ui) {
//         for root_scope_idx in self.vcd.root_scopes_by_idx() {
//             self.draw_selectable_child_or_orphan_scope(root_scope_idx, ui);
//         }
//     }
//     fn draw_selectable_child_or_orphan_scope(
//         &mut self,
//         scope_idx: fastwave_backend::ScopeIdx,
//         ui: &mut egui::Ui,
//     ) {
//         let name = self.vcd.scope_name_by_idx(scope_idx);
//         let fastwave_backend::ScopeIdx(idx) = scope_idx;
//         if self.vcd.child_scopes_by_idx(scope_idx).is_empty() {
//             if ui
//                 .add(egui::SelectableLabel::new(
//                     self.selected_module == scope_idx,
//                     name,
//                 ))
//                 .clicked()
//             {
//                 self.selected_module = scope_idx;
//             }
//         } else {
//             egui::collapsing_header::CollapsingState::load_with_default_open(
//                 ui.ctx(),
//                 egui::Id::new(idx),
//                 false,
//             )
//             .show_header(ui, |ui| {
//                 ui.with_layout(
//                     egui::Layout::top_down(egui::Align::LEFT).with_cross_justify(true),
//                     |ui| {
//                         if ui
//                             .add(egui::SelectableLabel::new(
//                                 self.selected_module == scope_idx,
//                                 name,
//                             ))
//                             .clicked()
//                         {
//                             self.selected_module = scope_idx;
//                         }
//                     },
//                 );
//             })
//             .body(|ui| self.draw_root_scope_view(scope_idx, ui));
//         }
//     }
//     fn draw_root_scope_view(&mut self, root_idx: fastwave_backend::ScopeIdx, ui: &mut egui::Ui) {
//         for child_scope_idx in self.vcd.child_scopes_by_idx(root_idx) {
//             self.draw_selectable_child_or_orphan_scope(child_scope_idx, ui);
//         }
//     }
// }
