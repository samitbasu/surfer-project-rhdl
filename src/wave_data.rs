use std::collections::HashMap;

use color_eyre::eyre::WrapErr;
use log::{error, warn};
use num::bigint::ToBigInt;
use num::{BigInt, BigUint, Zero};
use serde::{Deserialize, Serialize};

use crate::wave_container::VariableValue;
use crate::wave_source::WaveFormat;
use crate::{
    displayed_item::{
        DisplayedDivider, DisplayedItem, DisplayedItemRef, DisplayedTimeLine, DisplayedVariable,
    },
    translation::{TranslationPreference, Translator, TranslatorList},
    variable_name_type::VariableNameType,
    view::ItemDrawingInfo,
    viewport::Viewport,
    wave_container::{FieldRef, ScopeRef, VariableMeta, VariableRef, WaveContainer},
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
    pub active_scope: Option<ScopeRef>,
    /// Root items (variables, dividers, ...) to display
    pub displayed_items_order: Vec<DisplayedItemRef>,
    pub displayed_items: HashMap<DisplayedItemRef, DisplayedItem>,
    /// Tracks the consecutive displayed item refs
    pub display_item_ref_counter: DisplayedItemRef,
    pub viewports: Vec<Viewport>,
    /// Name of the translator used to translate this trace
    pub variable_format: HashMap<FieldRef, String>,
    pub cursor: Option<BigInt>,
    /// When right clicking we'll create a temporary cursor that shows where right click
    /// actions will apply. This gets cleared when the context menu is closed
    pub right_cursor: Option<BigInt>,
    pub markers: HashMap<u8, BigInt>,
    pub focused_item: Option<usize>,
    pub default_variable_name_type: VariableNameType,
    pub scroll_offset: f32,
    /// These are just stored during operation, so no need to serialize
    #[serde(skip)]
    pub drawing_infos: Vec<ItemDrawingInfo>,
    #[serde(skip)]
    pub top_item_draw_offset: f32,
    #[serde(skip)]
    pub total_height: f32,
    /// used by the `update_viewports` method after loading a new file
    #[serde(skip)]
    pub old_num_timestamps: Option<BigInt>,
}

impl WaveData {
    pub fn update_with(
        mut self,
        new_waves: Box<WaveContainer>,
        source: WaveSource,
        format: WaveFormat,
        translators: &TranslatorList,
        keep_unavailable: bool,
    ) -> WaveData {
        let active_scope = self
            .active_scope
            .take()
            .filter(|m| new_waves.scope_exists(m));
        let display_items = self.update_displayed_items(
            &new_waves,
            &self.displayed_items,
            keep_unavailable,
            translators,
        );

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
                display_items.values().any(|di| match di {
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

        let old_num_timestamps = Some(self.num_timestamps());
        let mut new_wave = WaveData {
            inner: *new_waves,
            source,
            format,
            active_scope,
            displayed_items_order: self.displayed_items_order,
            displayed_items: display_items,
            display_item_ref_counter: self.display_item_ref_counter,
            viewports: self.viewports,
            variable_format,
            cursor: self.cursor.clone(),
            right_cursor: None,
            markers: self.markers.clone(),
            focused_item: self.focused_item,
            default_variable_name_type: self.default_variable_name_type,
            scroll_offset: self.scroll_offset,
            drawing_infos: vec![],
            top_item_draw_offset: 0.,
            total_height: 0.,
            old_num_timestamps,
        };
        nested_format.retain(|nested, _| {
            let Some(variable_ref) = new_wave.displayed_items.values().find_map(|di| match di {
                DisplayedItem::Variable(DisplayedVariable { variable_ref, .. }) => {
                    Some(variable_ref)
                }
                _ => None,
            }) else {
                return false;
            };
            let meta = new_wave.inner.variable_meta(&nested.root).unwrap();
            new_wave
                .variable_translator(&FieldRef::without_fields(variable_ref.clone()), translators)
                .variable_info(&meta)
                .map(|info| info.has_subpath(&nested.field))
                .unwrap_or(false)
        });
        new_wave.variable_format.extend(nested_format);

        // load variables that need to be displayed
        let variables = new_wave
            .displayed_items
            .values()
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

    /// Needs to be called after update_with, once the new number of timestamps is available in
    /// the inner WaveContainer.
    pub fn update_viewports(&mut self) {
        if let Some(old_num_timestamps) = std::mem::take(&mut self.old_num_timestamps) {
            // FIXME: I'm not sure if Defaulting to 1 time step is the right thing to do if we
            // have none, but it does avoid some potentially nasty division by zero problems
            let new_num_timestamps = self
                .inner
                .max_timestamp()
                .unwrap_or_else(|| BigUint::from(1u32))
                .to_bigint()
                .unwrap();
            if new_num_timestamps != old_num_timestamps {
                for viewport in self.viewports.iter_mut() {
                    *viewport = viewport.clip_to(&old_num_timestamps, &new_num_timestamps);
                }
            }
        }
    }

    fn update_displayed_items(
        &self,
        new_waves: &WaveContainer,
        current_items: &HashMap<DisplayedItemRef, DisplayedItem>,
        keep_unavailable: bool,
        translators: &TranslatorList,
    ) -> HashMap<DisplayedItemRef, DisplayedItem> {
        current_items
            .iter()
            .filter_map(|(id, i)| {
                match i {
                    // keep without a change
                    DisplayedItem::Divider(_)
                    | DisplayedItem::Marker(_)
                    | DisplayedItem::TimeLine(_) => Some((*id, i.clone())),
                    DisplayedItem::Variable(s) => {
                        s.update(new_waves, keep_unavailable).map(|r| (*id, r))
                    }
                    DisplayedItem::Placeholder(p) => {
                        match new_waves.update_variable_ref(&p.variable_ref) {
                            None => {
                                if keep_unavailable {
                                    Some((*id, DisplayedItem::Placeholder(p.clone())))
                                } else {
                                    None
                                }
                            }
                            Some(new_variable_ref) => {
                                let Ok(meta) = new_waves
                                    .variable_meta(&new_variable_ref)
                                    .context("When updating")
                                    .map_err(|e| error!("{e:#?}"))
                                else {
                                    return Some((*id, DisplayedItem::Placeholder(p.clone())));
                                };
                                let translator = self.variable_translator(
                                    &FieldRef::without_fields(p.variable_ref.clone()),
                                    translators,
                                );
                                let info = translator.variable_info(&meta).unwrap();
                                Some((
                                    *id,
                                    DisplayedItem::Variable(
                                        p.clone().to_variable(info, new_variable_ref),
                                    ),
                                ))
                            }
                        }
                    }
                }
            })
            .collect()
    }

    pub fn select_preferred_translator(
        &self,
        var: VariableMeta,
        translators: &TranslatorList,
    ) -> String {
        translators
            .all_translators()
            .iter()
            .filter_map(|t| match t.translates(&var) {
                Ok(TranslationPreference::Prefer) => Some(t.name()),
                Ok(TranslationPreference::Yes) => None,
                Ok(TranslationPreference::No) => None,
                Err(e) => {
                    error!(
                        "Failed to check if {} translates {}\n{e:#?}",
                        t.name(),
                        var.var.full_path_string()
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

    pub fn add_variable(
        &mut self,
        translators: &TranslatorList,
        variable: &VariableRef,
    ) -> Option<DisplayedItemRef> {
        let Ok(meta) = self
            .inner
            .load_variables([variable].into_iter())
            .and_then(|_| self.inner.variable_meta(variable))
            .context("When adding variable")
            .map_err(|e| error!("{e:#?}"))
        else {
            return None;
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

        let id = self.insert_item(new_variable, None);
        self.compute_variable_display_names();
        Some(id)
    }

    pub fn remove_displayed_item(&mut self, count: usize, idx: usize) {
        for _ in 0..count {
            let visible_items_len = self.displayed_items_order.len();
            if let Some(DisplayedItem::Marker(marker)) = self
                .displayed_items_order
                .get(idx)
                .and_then(|id| self.displayed_items.get(id))
            {
                self.markers.remove(&marker.idx);
            }
            if visible_items_len > 0 && idx <= (visible_items_len - 1) {
                let displayed_item_id = self.displayed_items_order[idx];
                self.displayed_items_order.remove(idx);
                self.displayed_items.remove(&displayed_item_id);
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
    fn insert_item(&mut self, new_item: DisplayedItem, vidx: Option<usize>) -> DisplayedItemRef {
        if let Some(current_idx) = vidx {
            let insert_idx = current_idx + 1;
            let id = self.next_displayed_item_ref();
            self.displayed_items_order.insert(insert_idx, id);
            self.displayed_items.insert(id, new_item);
            id
        } else if let Some(focus_idx) = self.focused_item {
            let insert_idx = focus_idx + 1;
            let id = self.next_displayed_item_ref();
            self.displayed_items_order.insert(insert_idx, id);
            self.displayed_items.insert(id, new_item);
            self.focused_item = Some(insert_idx);
            id
        } else {
            let id = self.next_displayed_item_ref();
            self.displayed_items_order.push(id);
            self.displayed_items.insert(id, new_item);
            id
        }
    }

    pub fn go_to_cursor_if_not_in_view(&mut self) -> bool {
        if let Some(cursor) = &self.cursor {
            let num_timestamps = self.num_timestamps();
            self.viewports[0].go_to_cursor_if_not_in_view(cursor, &num_timestamps)
        } else {
            false
        }
    }

    #[inline]
    pub fn numbered_marker_location(&self, idx: u8, viewport: &Viewport, view_width: f32) -> f32 {
        viewport.pixel_from_time(
            self.numbered_marker_time(idx),
            view_width,
            &self.num_timestamps(),
        )
    }

    #[inline]
    pub fn numbered_marker_time(&self, idx: u8) -> &BigInt {
        self.markers.get(&idx).unwrap()
    }

    pub fn viewport_all(&self) -> Viewport {
        Viewport::new()
    }

    pub fn remove_placeholders(&mut self) {
        self.displayed_items.retain(|_, item| match item {
            DisplayedItem::Placeholder(_) => false,
            _ => true,
        });
        let remaining_ids = self.displayed_items_order.clone();
        self.displayed_items_order
            .retain(|i| remaining_ids.contains(i));
    }

    #[inline]
    pub fn any_displayed(&self) -> bool {
        !self.displayed_items.is_empty()
    }

    pub fn get_top_item(&self) -> usize {
        let default = if self.drawing_infos.is_empty() {
            0
        } else {
            self.drawing_infos.len() - 1
        };
        self.drawing_infos
            .iter()
            .enumerate()
            .find(|(_, di)| di.top() >= self.top_item_draw_offset - 1.) // Subtract a bit of margin to avoid floating-point errors
            .map(|(idx, _)| idx)
            .unwrap_or(default)
    }

    pub fn get_item_at_y(&self, y: f32) -> Option<usize> {
        if self.drawing_infos.is_empty() {
            return None;
        }
        let first_element_top = self.drawing_infos.first().unwrap().top();
        let first_element_bottom = self.drawing_infos.last().unwrap().bottom();
        let threshold = y + first_element_top + self.scroll_offset;
        if first_element_bottom <= threshold {
            return None;
        }
        self.drawing_infos
            .iter()
            .enumerate()
            .rev()
            .find(|(_, di)| di.top() <= threshold)
            .map(|(idx, _)| idx)
    }

    pub fn scroll_to_item(&mut self, idx: usize) {
        if self.drawing_infos.is_empty() {
            return;
        }
        // Set scroll_offset to different between requested element and first element
        let first_element_y = self.drawing_infos.first().unwrap().top();
        let item_y = self
            .drawing_infos
            .get(idx)
            .unwrap_or_else(|| self.drawing_infos.last().unwrap())
            .top();
        self.scroll_offset = item_y - first_element_y;
    }
    pub fn set_cursor_at_transition(
        &mut self,
        next: bool,
        variable: Option<usize>,
        skip_zero: bool,
    ) {
        if let Some(vidx) = variable.or(self.focused_item) {
            if let Some(cursor) = &self.cursor {
                if let Some(DisplayedItem::Variable(variable)) = &self
                    .displayed_items_order
                    .get(vidx)
                    .and_then(|id| self.displayed_items.get(id))
                {
                    if let Ok(Some(res)) = self.inner.query_variable(
                        &variable.variable_ref,
                        &cursor.to_biguint().unwrap_or_default(),
                    ) {
                        if next {
                            if let Some(ref time) = res.next {
                                let stime = time.to_bigint();
                                if stime.is_some() {
                                    self.cursor = stime.clone();
                                }
                            } else {
                                // No next transition, go to end
                                self.cursor = Some(self.num_timestamps().clone());
                            }
                        } else {
                            if let Some(stime) = res.current.unwrap().0.to_bigint() {
                                let bigone = BigInt::from(1);
                                // Check if we are on a transition
                                if stime == *cursor && *cursor >= bigone {
                                    // If so, subtract cursor position by one
                                    if let Ok(Some(newres)) = self.inner.query_variable(
                                        &variable.variable_ref,
                                        &(cursor - bigone).to_biguint().unwrap_or_default(),
                                    ) {
                                        if let Some(newstime) =
                                            newres.current.unwrap().0.to_bigint()
                                        {
                                            self.cursor = Some(newstime);
                                        }
                                    }
                                } else {
                                    self.cursor = Some(stime);
                                }
                            }
                        }

                        // if zero edges should be skipped
                        if skip_zero {
                            // check if the next transition is 0, if so and requested, go to
                            // next positive transition
                            if let Some(time) = &self.cursor {
                                let next_value = self.inner.query_variable(
                                    &variable.variable_ref,
                                    &time.to_biguint().unwrap_or_default(),
                                );
                                if next_value.is_ok_and(|r| {
                                    r.is_some_and(|r| {
                                        r.current.is_some_and(|v| match v.1 {
                                            VariableValue::BigUint(v) => v == BigUint::from(0u8),
                                            _ => false,
                                        })
                                    })
                                }) {
                                    self.set_cursor_at_transition(next, Some(vidx), false);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn next_displayed_item_ref(&mut self) -> usize {
        self.display_item_ref_counter += 1;
        self.display_item_ref_counter
    }

    /// Returns the number of timestamps in the current waves. For now, this adjusts the
    /// number of timestamps as returned by wave sources if they specify 0 timestamps. This is
    /// done to avoid having to consider what happens with the viewport. In the future,
    /// we should probably make this an Option<BigInt>
    pub fn num_timestamps(&self) -> BigInt {
        self.inner
            .max_timestamp()
            .and_then(|r| if r == BigUint::zero() { None } else { Some(r) })
            .unwrap_or(BigUint::from(1u32))
            .to_bigint()
            .unwrap()
    }
}
