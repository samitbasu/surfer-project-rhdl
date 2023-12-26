use std::collections::HashMap;

use color_eyre::eyre::WrapErr;
use eframe::epaint::Vec2;
use itertools::Itertools;
use log::{error, warn};
use num::{BigInt, ToPrimitive};
use serde::{Deserialize, Serialize};

use crate::{
    displayed_item::{DisplayedDivider, DisplayedItem, DisplayedSignal, DisplayedTimeLine},
    signal_name_type::SignalNameType,
    translation::{TranslationPreference, Translator, TranslatorList},
    viewport::Viewport,
    wave_container::{FieldRef, ModuleRef, SignalMeta, SignalRef, WaveContainer},
    wave_source::WaveSource,
};

pub const PER_SCROLL_EVENT: f32 = 50.0;
pub const SCROLL_EVENTS_PER_PAGE: f32 = 20.0;

#[derive(Serialize, Deserialize)]
pub struct WaveData {
    #[serde(skip, default = "WaveContainer::__new_empty")]
    pub inner: WaveContainer,
    pub source: WaveSource,
    pub active_module: Option<ModuleRef>,
    /// Root items (signals, dividers, ...) to display
    pub displayed_items: Vec<DisplayedItem>,
    pub viewports: Vec<Viewport>,
    pub num_timestamps: BigInt,
    /// Name of the translator used to translate this trace
    pub signal_format: HashMap<FieldRef, String>,
    pub cursor: Option<BigInt>,
    pub cursors: HashMap<u8, BigInt>,
    pub focused_item: Option<usize>,
    pub default_signal_name_type: SignalNameType,
    pub scroll: usize,
}

impl WaveData {
    pub fn update_with(
        mut self,
        new_waves: Box<WaveContainer>,
        source: WaveSource,
        num_timestamps: BigInt,
        wave_viewport: Vec<Viewport>,
        translators: &TranslatorList,
    ) -> WaveData {
        let active_module = self
            .active_module
            .take()
            .filter(|m| new_waves.module_exists(m));
        let display_items = self
            .displayed_items
            .drain(..)
            .filter(|i| match i {
                DisplayedItem::Signal(s) => new_waves.signal_exists(&s.signal_ref),
                DisplayedItem::Divider(_) => true,
                DisplayedItem::Cursor(_) => true,
                DisplayedItem::TimeLine(_) => true,
            })
            .collect::<Vec<_>>();
        let mut nested_format = self
            .signal_format
            .iter()
            .filter(|&(field_ref, _)| !field_ref.field.is_empty())
            .map(|(x, y)| (x.clone(), y.clone()))
            .collect::<HashMap<_, _>>();
        let signal_format = self
            .signal_format
            .drain()
            .filter(|(field_ref, candidate)| {
                display_items.iter().any(|di| match di {
                    DisplayedItem::Signal(DisplayedSignal { signal_ref, .. }) => {
                        let Ok(meta) = new_waves.signal_meta(signal_ref) else {
                            return false;
                        };
                        field_ref.field.is_empty()
                            && *signal_ref == field_ref.root
                            && translators.is_valid_translator(&meta, candidate.as_str())
                    }
                    _ => false,
                })
            })
            .collect();
        let mut new_wave = WaveData {
            inner: *new_waves,
            source,
            active_module,
            displayed_items: display_items,
            viewports: self
                .viewports
                .into_iter()
                .enumerate()
                .map(|(idx, viewport)| viewport.clip_to(&wave_viewport[idx]))
                .collect_vec(),
            signal_format,
            num_timestamps,
            cursor: self.cursor.clone(),
            cursors: self.cursors.clone(),
            focused_item: self.focused_item,
            default_signal_name_type: self.default_signal_name_type,
            scroll: self.scroll,
        };
        nested_format.retain(|nested, _| {
            let Some(signal_ref) = new_wave.displayed_items.iter().find_map(|di| match di {
                DisplayedItem::Signal(DisplayedSignal { signal_ref, .. }) => Some(signal_ref),
                _ => None,
            }) else {
                return false;
            };
            let meta = new_wave.inner.signal_meta(&nested.root).unwrap();
            new_wave
                .signal_translator(
                    &FieldRef {
                        root: signal_ref.clone(),
                        field: vec![],
                    },
                    translators,
                )
                .signal_info(&meta)
                .map(|info| info.has_subpath(&nested.field))
                .unwrap_or(false)
        });
        new_wave.signal_format.extend(nested_format);
        new_wave
    }

    pub fn select_preferred_translator(
        &self,
        sig: SignalMeta,
        translators: &TranslatorList,
    ) -> String {
        translators
            .all_translators()
            .iter()
            .filter_map(|t| match t.translates(&sig) {
                Ok(TranslationPreference::Prefer) => Some(t.name()),
                Ok(TranslationPreference::Yes) => None,
                Ok(TranslationPreference::No) => None,
                Err(e) => {
                    error!(
                        "Failed to check if {} translates {}\n{e:#?}",
                        t.name(),
                        sig.sig.full_path_string()
                    );
                    None
                }
            })
            .next()
            .unwrap_or(translators.default.clone())
    }

    pub fn signal_translator<'a>(
        &'a self,
        field: &FieldRef,
        translators: &'a TranslatorList,
    ) -> &'a dyn Translator {
        let translator_name = self.signal_format.get(field).cloned().unwrap_or_else(|| {
            if field.field.is_empty() {
                self.inner
                    .signal_meta(&field.root)
                    .map(|meta| self.select_preferred_translator(meta, translators))
                    .unwrap_or_else(|e| {
                        warn!("{e:#?}");
                        translators.default.clone()
                    })
            } else {
                translators.default.clone()
            }
        });
        let translator = translators.get_translator(&translator_name);
        translator
    }

    pub fn handle_canvas_zoom(
        &mut self,
        // Canvas relative
        mouse_ptr_timestamp: Option<f64>,
        delta: f64,
        viewport_idx: usize,
    ) {
        // Zoom or scroll
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self.viewports[viewport_idx];

        let (target_left, target_right) = match mouse_ptr_timestamp {
            Some(mouse_location) => (
                (left - mouse_location) / delta + mouse_location,
                (right - mouse_location) / delta + mouse_location,
            ),
            None => {
                let mid_point = (right + left) * 0.5;
                let offset = (right - left) * delta * 0.5;

                (mid_point - offset, mid_point + offset)
            }
        };

        self.viewports[viewport_idx].curr_left = target_left;
        self.viewports[viewport_idx].curr_right = target_right;
    }

    pub fn add_signal(&mut self, translators: &TranslatorList, sig: &SignalRef) {
        let Ok(meta) = self
            .inner
            .signal_meta(sig)
            .context("When adding signal")
            .map_err(|e| error!("{e:#?}"))
        else {
            return;
        };

        let translator =
            self.signal_translator(&FieldRef::without_fields(sig.clone()), translators);
        let info = translator.signal_info(&meta).unwrap();

        let new_signal = DisplayedItem::Signal(DisplayedSignal {
            signal_ref: sig.clone(),
            info,
            color: None,
            background_color: None,
            display_name: sig.name.clone(),
            display_name_type: self.default_signal_name_type,
            manual_name: None,
        });

        self.insert_item(new_signal, None);
        self.compute_signal_display_names();
    }

    pub fn remove_displayed_item(&mut self, count: usize, idx: usize) {
        for _ in 0..count {
            let visible_signals_len = self.displayed_items.len();
            if let Some(DisplayedItem::Cursor(cursor)) = self.displayed_items.get(idx) {
                self.cursors.remove(&cursor.idx);
            }
            if visible_signals_len > 0 && idx <= (visible_signals_len - 1) {
                self.displayed_items.remove(idx);
                if let Some(focused) = self.focused_item {
                    if focused == idx {
                        if (idx > 0) && (idx == (visible_signals_len - 1)) {
                            // if the end of list is selected
                            self.focused_item = Some(idx - 1);
                        }
                    } else if idx < focused {
                        self.focused_item = Some(focused - 1)
                    }
                    if self.displayed_items.is_empty() {
                        self.focused_item = None;
                    }
                }
            }
        }
        self.compute_signal_display_names();
    }

    pub fn add_divider(&mut self, name: Option<String>, vidx: Option<usize>) {
        self.insert_item(
            DisplayedItem::Divider(DisplayedDivider {
                color: None,
                background_color: None,
                name,
            }),
            vidx,
        );
    }

    pub fn add_timeline(&mut self, vidx: Option<usize>) {
        self.insert_item(
            DisplayedItem::TimeLine(DisplayedTimeLine {
                color: None,
                background_color: None,
                name: None,
            }),
            vidx,
        );
    }

    /// Insert item after item vidx if Some(vidx).
    /// If None, insert after focused item if there is one, otherwise insert at the end.
    /// Focus on the inserted item if there was a focues item.
    fn insert_item(&mut self, new_item: DisplayedItem, vidx: Option<usize>) {
        if let Some(current_idx) = vidx {
            let insert_idx = current_idx + 1;
            self.displayed_items.insert(insert_idx, new_item);
        } else if let Some(focus_idx) = self.focused_item {
            let insert_idx = focus_idx + 1;
            self.displayed_items.insert(insert_idx, new_item);
            self.focused_item = Some(insert_idx);
        } else {
            self.displayed_items.push(new_item);
        }
    }

    pub fn go_to_start(&mut self, viewport_idx: usize) {
        let width =
            self.viewports[viewport_idx].curr_right - self.viewports[viewport_idx].curr_left;

        self.viewports[viewport_idx].curr_left = 0.0;
        self.viewports[viewport_idx].curr_right = width;
    }

    pub fn go_to_end(&mut self, viewport_idx: usize) {
        let end_point = self.num_timestamps.clone().to_f64().unwrap();
        let width =
            self.viewports[viewport_idx].curr_right - self.viewports[viewport_idx].curr_left;

        self.viewports[viewport_idx].curr_left = end_point - width;
        self.viewports[viewport_idx].curr_right = end_point;
    }

    pub fn zoom_to_fit(&mut self, viewport_idx: usize) {
        self.viewports[viewport_idx].curr_left = 0.0;
        self.viewports[viewport_idx].curr_right = self.num_timestamps.clone().to_f64().unwrap();
    }

    pub fn go_to_time(&mut self, center: &BigInt, viewport_idx: usize) {
        let center_point = center.to_f64().unwrap();
        let half_width =
            (self.viewports[viewport_idx].curr_right - self.viewports[viewport_idx].curr_left) / 2.;

        self.viewports[viewport_idx].curr_left = center_point - half_width;
        self.viewports[viewport_idx].curr_right = center_point + half_width;
    }

    pub fn handle_canvas_scroll(
        &mut self,
        // Canvas relative
        delta: Vec2,
        viewport_idx: usize,
    ) {
        // Scroll 1/SCROLL_EVENTS_PER_PAGE = 5% of the viewport per scroll event.
        // One scroll event yields PER_SCROLL_EVENT = 50
        let scroll_step = -(self.viewports[viewport_idx].curr_right
            - self.viewports[viewport_idx].curr_left)
            / (PER_SCROLL_EVENT * SCROLL_EVENTS_PER_PAGE) as f64;

        let target_left = self.viewports[viewport_idx].curr_left + scroll_step * delta.y as f64;
        let target_right = self.viewports[viewport_idx].curr_right + scroll_step * delta.y as f64;

        self.viewports[viewport_idx].curr_left = target_left;
        self.viewports[viewport_idx].curr_right = target_right;
    }
}
