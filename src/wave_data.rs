use std::collections::HashMap;

use color_eyre::eyre::WrapErr;
use log::{error, warn};
use num::BigInt;

use crate::{
    displayed_item::{DisplayedDivider, DisplayedItem, DisplayedSignal},
    signal_name_type::SignalNameType,
    translation::{TranslationPreference, Translator, TranslatorList},
    viewport::Viewport,
    wave_container::{FieldRef, ModuleRef, SignalMeta, SignalRef, WaveContainer},
    wave_source::WaveSource,
};

pub struct WaveData {
    pub inner: WaveContainer,
    pub source: WaveSource,
    pub active_module: Option<ModuleRef>,
    /// Root items (signals, dividers, ...) to display
    pub displayed_items: Vec<DisplayedItem>,
    pub viewport: Viewport,
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
        wave_viewport: Viewport,
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
            viewport: self.viewport.clone().clip_to(&wave_viewport),
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
    ) {
        // Zoom or scroll
        let Viewport {
            curr_left: left,
            curr_right: right,
            ..
        } = &self.viewport;

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

        self.viewport.curr_left = target_left;
        self.viewport.curr_right = target_right;
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

        self.displayed_items
            .push(DisplayedItem::Signal(DisplayedSignal {
                signal_ref: sig.clone(),
                info,
                color: None,
                background_color: None,
                display_name: sig.name.clone(),
                display_name_type: self.default_signal_name_type,
            }));
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
                    } else {
                        if idx < focused {
                            self.focused_item = Some(focused - 1)
                        }
                    }
                    if self.displayed_items.is_empty() {
                        self.focused_item = None;
                    }
                }
            }
        }
        self.compute_signal_display_names();
    }

    pub fn add_divider(&mut self, name: String) {
        self.displayed_items
            .push(DisplayedItem::Divider(DisplayedDivider {
                color: None,
                background_color: None,
                name,
            }))
    }
}
