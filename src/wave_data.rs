use std::collections::HashMap;

use color_eyre::eyre::WrapErr;
use eframe::epaint::Vec2;
use log::{error, warn};
use num::{BigInt, ToPrimitive};
use serde::{Deserialize, Serialize};

use crate::wave_source::WaveFormat;
use crate::{
    displayed_item::{DisplayedDivider, DisplayedItem, DisplayedTimeLine, DisplayedVariable},
    translation::{TranslationPreference, Translator, TranslatorList},
    variable_name_type::VariableNameType,
    view::ItemDrawingInfo,
    viewport::Viewport,
    wave_container::{FieldRef, ModuleRef, VariableMeta, VariableRef, WaveContainer},
    wave_source::WaveSource,
};

pub const PER_SCROLL_EVENT: f32 = 50.0;
pub const SCROLL_EVENTS_PER_PAGE: f32 = 20.0;

#[derive(Serialize, Deserialize)]
pub struct WaveData {
    #[serde(skip, default = "WaveContainer::__new_empty")]
    pub inner: WaveContainer,
    pub source: WaveSource,
    pub format: WaveFormat,
    pub active_module: Option<ModuleRef>,
    /// Root items (variables, dividers, ...) to display
    pub displayed_items: Vec<DisplayedItem>,
    pub viewport: Viewport,
    pub num_timestamps: BigInt,
    /// Name of the translator used to translate this trace
    pub variable_format: HashMap<FieldRef, String>,
    pub cursor: Option<BigInt>,
    pub cursors: HashMap<u8, BigInt>,
    pub focused_item: Option<usize>,
    pub default_variable_name_type: VariableNameType,
    pub scroll_offset: f32,
    /// These are just stored during operation, so no need to serialize
    #[serde(skip)]
    pub item_offsets: Vec<ItemDrawingInfo>,
    #[serde(skip)]
    pub top_item_draw_offset: f32,
    #[serde(skip)]
    pub total_height: f32,
}

impl WaveData {
    pub fn update_with(
        mut self,
        new_waves: Box<WaveContainer>,
        source: WaveSource,
        format: WaveFormat,
        num_timestamps: BigInt,
        wave_viewport: Viewport,
        translators: &TranslatorList,
        keep_unavailable: bool,
    ) -> WaveData {
        let active_module = self
            .active_module
            .take()
            .filter(|m| new_waves.module_exists(m));
        let mut current_items = self.displayed_items.clone();
        let display_items = current_items
            .drain(..)
            .filter_map(|i| match i {
                DisplayedItem::Divider(_)
                | DisplayedItem::Cursor(_)
                | DisplayedItem::TimeLine(_) => Some(i),
                DisplayedItem::Variable(s) => {
                    if new_waves.variable_exists(&s.variable_ref) {
                        Some(DisplayedItem::Variable(s))
                    } else if keep_unavailable {
                        Some(DisplayedItem::Placeholder(s.to_placeholder()))
                    } else {
                        None
                    }
                }
                DisplayedItem::Placeholder(p) => {
                    if new_waves.variable_exists(&p.variable_ref) {
                        let Ok(meta) = new_waves
                            .variable_meta(&p.variable_ref)
                            .context("When updating")
                            .map_err(|e| error!("{e:#?}"))
                        else {
                            return Some(DisplayedItem::Placeholder(p));
                        };
                        let translator = self.variable_translator(
                            &FieldRef::without_fields(p.variable_ref.clone()),
                            translators,
                        );
                        let info = translator.variable_info(&meta).unwrap();
                        Some(DisplayedItem::Variable(p.to_variable(info)))
                    } else if keep_unavailable {
                        Some(DisplayedItem::Placeholder(p))
                    } else {
                        None
                    }
                }
            })
            .collect::<Vec<_>>();
        let mut nested_format = self
            .variable_format
            .iter()
            .filter(|&(field_ref, _)| !field_ref.field.is_empty())
            .map(|(x, y)| (x.clone(), y.clone()))
            .collect::<HashMap<_, _>>();
        let variable_format = self
            .variable_format
            .drain()
            .filter(|(field_ref, candidate)| {
                display_items.iter().any(|di| match di {
                    DisplayedItem::Variable(DisplayedVariable { variable_ref, .. }) => {
                        let Ok(meta) = new_waves.variable_meta(variable_ref) else {
                            return false;
                        };
                        field_ref.field.is_empty()
                            && *variable_ref == field_ref.root
                            && translators.is_valid_translator(&meta, candidate.as_str())
                    }
                    _ => false,
                })
            })
            .collect();
        let mut new_wave = WaveData {
            inner: *new_waves,
            source,
            format,
            active_module,
            displayed_items: display_items,
            viewport: self.viewport.clone().clip_to(&wave_viewport),
            variable_format,
            num_timestamps,
            cursor: self.cursor.clone(),
            cursors: self.cursors.clone(),
            focused_item: self.focused_item,
            default_variable_name_type: self.default_variable_name_type,
            scroll_offset: self.scroll_offset,
            item_offsets: vec![],
            top_item_draw_offset: 0.,
            total_height: 0.,
        };
        nested_format.retain(|nested, _| {
            let Some(variable_ref) = new_wave.displayed_items.iter().find_map(|di| match di {
                DisplayedItem::Variable(DisplayedVariable { variable_ref, .. }) => {
                    Some(variable_ref)
                }
                _ => None,
            }) else {
                return false;
            };
            let meta = new_wave.inner.variable_meta(&nested.root).unwrap();
            new_wave
                .variable_translator(
                    &FieldRef {
                        root: variable_ref.clone(),
                        field: vec![],
                    },
                    translators,
                )
                .variable_info(&meta)
                .map(|info| info.has_subpath(&nested.field))
                .unwrap_or(false)
        });
        new_wave.variable_format.extend(nested_format);

        // load variables that need to be displayed
        let variables = new_wave
            .displayed_items
            .iter()
            .filter_map(|item| match item {
                DisplayedItem::Variable(r) => Some(&r.variable_ref),
                _ => None,
            });
        new_wave
            .inner
            .load_variables(variables)
            .expect("internal error: failed to load variables");

        new_wave
    }

    pub fn select_preferred_translator(
        &self,
        sig: VariableMeta,
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
            .unwrap_or_else(|| translators.default.clone())
    }

    pub fn variable_translator<'a>(
        &'a self,
        field: &FieldRef,
        translators: &'a TranslatorList,
    ) -> &'a dyn Translator {
        let translator_name = self.variable_format.get(field).cloned().unwrap_or_else(|| {
            if field.field.is_empty() {
                self.inner
                    .variable_meta(&field.root)
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

    pub fn add_variable(&mut self, translators: &TranslatorList, variable: &VariableRef) {
        let Ok(meta) = self
            .inner
            .load_variable(variable)
            .context("When adding variable")
            .map_err(|e| error!("{e:#?}"))
        else {
            return;
        };

        let translator =
            self.variable_translator(&FieldRef::without_fields(variable.clone()), translators);
        let info = translator.variable_info(&meta).unwrap();

        let new_variable = DisplayedItem::Variable(DisplayedVariable {
            variable_ref: variable.clone(),
            info,
            color: None,
            background_color: None,
            display_name: variable.name.clone(),
            display_name_type: self.default_variable_name_type,
            manual_name: None,
        });

        self.insert_item(new_variable, None);
        self.compute_variable_display_names();
    }

    pub fn remove_displayed_item(&mut self, count: usize, idx: usize) {
        for _ in 0..count {
            let visible_items_len = self.displayed_items.len();
            if let Some(DisplayedItem::Cursor(cursor)) = self.displayed_items.get(idx) {
                self.cursors.remove(&cursor.idx);
            }
            if visible_items_len > 0 && idx <= (visible_items_len - 1) {
                self.displayed_items.remove(idx);
                if let Some(focused) = self.focused_item {
                    if focused == idx {
                        if (idx > 0) && (idx == (visible_items_len - 1)) {
                            // if the end of list is selected
                            self.focused_item = Some(idx - 1);
                        }
                    } else if idx < focused {
                        self.focused_item = Some(focused - 1)
                    }
                    if !self.any_displayed() {
                        self.focused_item = None;
                    }
                }
            }
        }
        self.compute_variable_display_names();
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

    pub fn go_to_start(&mut self) {
        let width = self.viewport.curr_right - self.viewport.curr_left;

        self.viewport.curr_left = 0.0;
        self.viewport.curr_right = width;
    }

    pub fn go_to_end(&mut self) {
        let end_point = self.num_timestamps.to_f64().unwrap();
        let width = self.viewport.curr_right - self.viewport.curr_left;

        self.viewport.curr_left = end_point - width;
        self.viewport.curr_right = end_point;
    }

    pub fn zoom_to_fit(&mut self) {
        self.viewport.curr_left = 0.0;
        self.viewport.curr_right = self.num_timestamps.to_f64().unwrap();
    }

    pub fn go_to_time(&mut self, center: &BigInt) {
        let center_point = center.to_f64().unwrap();
        let half_width = (self.viewport.curr_right - self.viewport.curr_left) / 2.;

        self.viewport.curr_left = center_point - half_width;
        self.viewport.curr_right = center_point + half_width;
    }

    #[inline]
    pub fn numbered_cursor_location(&self, idx: u8, viewport: &Viewport, view_width: f32) -> f32 {
        viewport.from_time(self.numbered_cursor_time(idx), view_width)
    }

    #[inline]
    pub fn numbered_cursor_time(&self, idx: u8) -> &BigInt {
        self.cursors.get(&idx).unwrap()
    }

    pub fn handle_canvas_scroll(
        &mut self,
        // Canvas relative
        delta: Vec2,
    ) {
        // Scroll 1/SCROLL_EVENTS_PER_PAGE = 5% of the viewport per scroll event.
        // One scroll event yields PER_SCROLL_EVENT = 50
        let scroll_step = -(self.viewport.curr_right - self.viewport.curr_left)
            / (PER_SCROLL_EVENT * SCROLL_EVENTS_PER_PAGE) as f64;

        let target_left = self.viewport.curr_left + scroll_step * delta.y as f64;
        let target_right = self.viewport.curr_right + scroll_step * delta.y as f64;

        self.viewport.curr_left = target_left;
        self.viewport.curr_right = target_right;
    }

    pub fn viewport_all(&self) -> Viewport {
        Viewport {
            curr_left: 0.,
            curr_right: self.num_timestamps.to_f64().unwrap_or(1.0),
        }
    }

    pub fn remove_placeholders(&mut self) {
        self.displayed_items.retain(|i| match i {
            DisplayedItem::Placeholder(_) => false,
            _ => true,
        })
    }

    #[inline]
    pub fn any_displayed(&self) -> bool {
        !self.displayed_items.is_empty()
    }

    pub fn get_top_item(&self) -> usize {
        let default = if self.item_offsets.is_empty() {
            0
        } else {
            self.item_offsets.len() - 1
        };
        self.item_offsets
            .iter()
            .enumerate()
            .find(|(_, di)| di.top() >= self.top_item_draw_offset - 1.) // Subtract a bit of margin to avoid floating-point errors
            .map(|(idx, _)| idx)
            .unwrap_or(default)
    }

    pub fn get_item_at_y(&self, y: f32) -> Option<usize> {
        if self.item_offsets.is_empty() {
            return None;
        }
        let first_element_top = self.item_offsets.first().unwrap().top();
        let first_element_bottom = self.item_offsets.last().unwrap().bottom();
        let threshold = y + first_element_top + self.scroll_offset;
        if first_element_bottom <= threshold {
            return None;
        }
        self.item_offsets
            .iter()
            .enumerate()
            .rev()
            .find(|(_, di)| di.top() <= threshold)
            .map(|(idx, _)| idx)
    }

    pub fn scroll_to_item(&mut self, idx: usize) {
        if self.item_offsets.is_empty() {
            return;
        }
        // Set scroll_offset to different between requested element and first element
        let first_element_y = self.item_offsets.first().unwrap().top();
        let item_y = self
            .item_offsets
            .get(idx)
            .unwrap_or_else(|| self.item_offsets.last().unwrap())
            .top();
        self.scroll_offset = item_y - first_element_y;
    }
}
