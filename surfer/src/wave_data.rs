use std::collections::HashMap;

use color_eyre::eyre::{Result, WrapErr};
use log::{error, info, warn};
use num::bigint::ToBigInt as _;
use num::{BigInt, BigUint, Zero};
use serde::{Deserialize, Serialize};
use surfer_translation_types::{TranslationPreference, Translator, VariableValue};

use crate::data_container::DataContainer;
use crate::displayed_item::{
    DisplayedDivider, DisplayedFieldRef, DisplayedItem, DisplayedItemIndex, DisplayedItemRef,
    DisplayedStream, DisplayedTimeLine, DisplayedVariable,
};
use crate::graphics::{Graphic, GraphicId};
use crate::transaction_container::{StreamScopeRef, TransactionRef, TransactionStreamRef};
use crate::translation::{DynTranslator, TranslatorList, VariableInfoExt};
use crate::variable_name_type::VariableNameType;
use crate::view::ItemDrawingInfo;
use crate::viewport::Viewport;
use crate::wave_container::{ScopeRef, VariableMeta, VariableRef, VariableRefExt, WaveContainer};
use crate::wave_source::{WaveFormat, WaveSource};
use crate::wellen::LoadSignalsCmd;
use ftr_parser::types::Transaction;
use std::fmt::Formatter;
use std::ops::Not;

pub const PER_SCROLL_EVENT: f32 = 50.0;
pub const SCROLL_EVENTS_PER_PAGE: f32 = 20.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScopeType {
    WaveScope(ScopeRef),
    StreamScope(StreamScopeRef),
}

impl std::fmt::Display for ScopeType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ScopeType::WaveScope(w) => w.fmt(f),
            ScopeType::StreamScope(s) => s.fmt(f),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct WaveData {
    #[serde(skip, default = "DataContainer::__new_empty")]
    pub inner: DataContainer,
    pub source: WaveSource,
    pub format: WaveFormat,
    pub active_scope: Option<ScopeType>,
    /// Root items (variables, dividers, ...) to display
    pub displayed_items_order: Vec<DisplayedItemRef>,
    pub displayed_items: HashMap<DisplayedItemRef, DisplayedItem>,
    /// Tracks the consecutive displayed item refs
    pub display_item_ref_counter: usize,
    pub viewports: Vec<Viewport>,
    pub cursor: Option<BigInt>,
    pub markers: HashMap<u8, BigInt>,
    pub focused_item: Option<DisplayedItemIndex>,
    pub focused_transaction: (Option<TransactionRef>, Option<Transaction>),
    pub selected_items: std::collections::HashSet<DisplayedItemRef>,
    pub default_variable_name_type: VariableNameType,
    pub scroll_offset: f32,
    pub display_variable_indices: bool,
    pub graphics: HashMap<GraphicId, Graphic>,
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

fn select_preferred_translator(var: &VariableMeta, translators: &TranslatorList) -> String {
    let mut preferred: Vec<_> = translators
        .all_translators()
        .iter()
        .filter_map(|t| match t.translates(var) {
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
        .collect();
    if preferred.len() > 1 {
        // For a single bit that has other preferred translators in addition to "Bit", like enum,
        // we would like to select the other one.
        let bit = "Bit".to_string();
        if var.num_bits == Some(1) {
            preferred.retain(|x| x != &bit);
        }
        if preferred.len() > 1 {
            warn!(
                "More than one preferred translator for variable {} in scope {}: {}",
                var.var.name,
                var.var.path.strs.join("."),
                preferred.join(", ")
            );
            preferred.sort();
        }
    }
    // make sure we always pick the same translator, at least
    preferred
        .pop()
        .unwrap_or_else(|| translators.default.clone())
}

pub fn variable_translator<'a, F>(
    translator: Option<&String>,
    field: &[String],
    translators: &'a TranslatorList,
    meta: F,
) -> &'a DynTranslator
where
    F: FnOnce() -> Result<VariableMeta>,
{
    let translator_name = translator
        .cloned()
        .or_else(|| {
            Some(if field.is_empty() {
                meta()
                    .as_ref()
                    .map(|meta| select_preferred_translator(meta, translators).clone())
                    .unwrap_or_else(|e| {
                        warn!("{e:#?}");
                        translators.default.clone()
                    })
            } else {
                translators.default.clone()
            })
        })
        .unwrap();

    let translator = translators.get_translator(&translator_name);
    translator
}

impl WaveData {
    pub fn update_with_waves(
        mut self,
        new_waves: Box<WaveContainer>,
        source: WaveSource,
        format: WaveFormat,
        translators: &TranslatorList,
        keep_unavailable: bool,
    ) -> (WaveData, Option<LoadSignalsCmd>) {
        let active_scope = self.active_scope.take().filter(|m| {
            if let ScopeType::WaveScope(w) = m {
                new_waves.scope_exists(w)
            } else {
                false
            }
        });
        let display_items = self.update_displayed_items(
            &new_waves,
            &self.displayed_items,
            keep_unavailable,
            translators,
        );

        let old_num_timestamps = Some(self.num_timestamps());
        let mut new_wavedata = WaveData {
            inner: DataContainer::Waves(*new_waves),
            source,
            format,
            active_scope,
            displayed_items_order: self.displayed_items_order,
            displayed_items: display_items,
            display_item_ref_counter: self.display_item_ref_counter,
            viewports: self.viewports,
            cursor: self.cursor.clone(),
            markers: self.markers.clone(),
            focused_item: self.focused_item,
            focused_transaction: self.focused_transaction,
            selected_items: std::collections::HashSet::new(),
            default_variable_name_type: self.default_variable_name_type,
            display_variable_indices: self.display_variable_indices,
            scroll_offset: self.scroll_offset,
            drawing_infos: vec![],
            top_item_draw_offset: 0.,
            graphics: HashMap::new(),
            total_height: 0.,
            old_num_timestamps,
        };

        new_wavedata.update_metadata(translators);
        let load_commands = new_wavedata.load_waves();
        (new_wavedata, load_commands)
    }

    pub fn update_with_items(
        &mut self,
        new_items: &HashMap<DisplayedItemRef, DisplayedItem>,
        order: Vec<DisplayedItemRef>,
        translators: &TranslatorList,
    ) -> Option<LoadSignalsCmd> {
        self.displayed_items_order = order;
        self.displayed_items = self.update_displayed_items(
            self.inner.as_waves().unwrap(),
            new_items,
            true,
            translators,
        );
        self.display_item_ref_counter = self
            .displayed_items
            .keys()
            .map(|dir| dir.0)
            .max()
            .unwrap_or(0);

        self.update_metadata(translators);
        self.load_waves()
    }

    /// Go through all signals and update the metadata for all signals
    ///
    /// Used after loading new waves, signals or switching a bunch of translators
    fn update_metadata(&mut self, translators: &TranslatorList) {
        for (_vidx, di) in self.displayed_items.iter_mut() {
            let DisplayedItem::Variable(displayed_variable) = di else {
                continue;
            };

            let meta = self
                .inner
                .as_waves()
                .unwrap()
                .variable_meta(&displayed_variable.variable_ref.clone())
                .unwrap();
            let translator =
                variable_translator(displayed_variable.get_format(&[]), &[], translators, || {
                    Ok(meta.clone())
                });
            let info = translator.variable_info(&meta).ok();

            match info {
                Some(info) => displayed_variable
                    .field_formats
                    .retain(|ff| info.has_subpath(&ff.field)),
                _ => displayed_variable.field_formats.clear(),
            }
        }
    }

    /// Get the underlying wave container to load all signals that are being displayed
    ///
    /// This is needed for wave containers that lazy-load signals.
    fn load_waves(&mut self) -> Option<LoadSignalsCmd> {
        let variables = self.displayed_items.values().filter_map(|item| match item {
            DisplayedItem::Variable(r) => Some(&r.variable_ref),
            _ => None,
        });
        self.inner
            .as_waves_mut()
            .unwrap()
            .load_variables(variables)
            .expect("internal error: failed to load variables")
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
        waves: &WaveContainer,
        items: &HashMap<DisplayedItemRef, DisplayedItem>,
        keep_unavailable: bool,
        translators: &TranslatorList,
    ) -> HashMap<DisplayedItemRef, DisplayedItem> {
        items
            .iter()
            .filter_map(|(id, i)| {
                match i {
                    // keep without a change
                    DisplayedItem::Divider(_)
                    | DisplayedItem::Marker(_)
                    | DisplayedItem::TimeLine(_)
                    | DisplayedItem::Stream(_) => Some((*id, i.clone())),
                    DisplayedItem::Variable(s) => {
                        s.update(waves, keep_unavailable).map(|r| (*id, r))
                    }
                    DisplayedItem::Placeholder(p) => {
                        match waves.update_variable_ref(&p.variable_ref) {
                            None => {
                                if keep_unavailable {
                                    Some((*id, DisplayedItem::Placeholder(p.clone())))
                                } else {
                                    None
                                }
                            }
                            Some(new_variable_ref) => {
                                let Ok(meta) = waves
                                    .variable_meta(&new_variable_ref)
                                    .context("When updating")
                                    .map_err(|e| error!("{e:#?}"))
                                else {
                                    return Some((*id, DisplayedItem::Placeholder(p.clone())));
                                };
                                let translator = variable_translator(
                                    p.format.as_ref(),
                                    &[],
                                    translators,
                                    || Ok(meta.clone()),
                                );
                                let info = translator.variable_info(&meta).unwrap();
                                Some((
                                    *id,
                                    DisplayedItem::Variable(
                                        p.clone().into_variable(info, new_variable_ref),
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
        select_preferred_translator(&var, translators)
    }

    pub fn variable_translator<'a>(
        &'a self,
        field: &DisplayedFieldRef,
        translators: &'a TranslatorList,
    ) -> &'a DynTranslator {
        let Some(DisplayedItem::Variable(displayed_variable)) =
            self.displayed_items.get(&field.item)
        else {
            panic!("asking for translator for a non DisplayItem::Variable item")
        };

        variable_translator(
            displayed_variable.get_format(&field.field),
            &field.field,
            translators,
            || {
                self.inner
                    .as_waves()
                    .unwrap()
                    .variable_meta(&displayed_variable.variable_ref)
            },
        )
    }

    pub fn add_variables(
        &mut self,
        translators: &TranslatorList,
        variables: Vec<VariableRef>,
    ) -> (Option<LoadSignalsCmd>, Vec<DisplayedItemRef>) {
        let mut indices = vec![];
        // load variables from waveform
        let res = match self
            .inner
            .as_waves_mut()
            .unwrap()
            .load_variables(variables.iter())
        {
            Err(e) => {
                error!("{e:#?}");
                return (None, indices);
            }
            Ok(res) => res,
        };

        // initialize translator and add display item
        for variable in variables {
            let Ok(meta) = self
                .inner
                .as_waves()
                .unwrap()
                .variable_meta(&variable)
                .context("When adding variable")
                .map_err(|e| error!("{e:#?}"))
            else {
                return (res, indices);
            };

            let translator = variable_translator(None, &[], translators, || Ok(meta.clone()));
            let info = translator.variable_info(&meta).unwrap();

            let new_variable = DisplayedItem::Variable(DisplayedVariable {
                variable_ref: variable.clone(),
                info,
                color: None,
                background_color: None,
                display_name: variable.name.clone(),
                display_name_type: self.default_variable_name_type,
                manual_name: None,
                format: None,
                field_formats: vec![],
            });

            indices.push(self.insert_item(new_variable, None));
        }

        self.compute_variable_display_names();
        (res, indices)
    }

    pub fn remove_displayed_item(&mut self, id: DisplayedItemRef) {
        if let Some(idx) = self
            .displayed_items_order
            .iter()
            .enumerate()
            .find(|(_, local_id)| **local_id == id)
        {
            if let Some(fidx) = self.focused_item {
                // move focus up if if signal was removed below focus
                if fidx.0 > idx.0 {
                    self.focused_item = Some(DisplayedItemIndex(
                        fidx.0
                            .saturating_sub(1)
                            .min(self.displayed_items_order.len().saturating_sub(2)),
                    ));
                // if the focus was removed move if it was the last element
                } else if fidx.0 == idx.0 {
                    self.focused_item = Some(DisplayedItemIndex(
                        fidx.0
                            .min(self.displayed_items_order.len().saturating_sub(2)),
                    ));
                }
                // when the removed item is above the focus, the focus does not move
            }
            self.displayed_items_order.remove(idx.0);
            self.displayed_items.remove(&id);
            if self.displayed_items_order.is_empty() {
                self.focused_item = None;
            }
        }
    }

    pub fn add_divider(&mut self, name: Option<String>, vidx: Option<DisplayedItemIndex>) {
        self.insert_item(
            DisplayedItem::Divider(DisplayedDivider {
                color: None,
                background_color: None,
                name,
            }),
            vidx,
        );
    }

    pub fn add_timeline(&mut self, vidx: Option<DisplayedItemIndex>) {
        self.insert_item(
            DisplayedItem::TimeLine(DisplayedTimeLine {
                color: None,
                background_color: None,
                name: None,
            }),
            vidx,
        );
    }

    pub fn add_generator(&mut self, gen_ref: TransactionStreamRef) {
        let Some(gen_id) = gen_ref.gen_id else { return };

        if self
            .inner
            .as_transactions()
            .unwrap()
            .get_generator(gen_id)
            .unwrap()
            .transactions
            .is_empty()
        {
            info!("(Generator {})Loading transactions into memory!", gen_id);
            match self
                .inner
                .as_transactions_mut()
                .unwrap()
                .inner
                .load_stream_into_memory(gen_ref.stream_id)
            {
                Ok(_) => info!("(Generator {}) Finished loading transactions!", gen_id),
                Err(_) => return,
            }
        }

        let gen = self
            .inner
            .as_transactions()
            .unwrap()
            .get_generator(gen_id)
            .unwrap();
        let mut last_times_on_row = vec![(BigUint::ZERO, BigUint::ZERO)];
        calculate_rows_of_stream(&gen.transactions, &mut last_times_on_row);

        let new_gen = DisplayedItem::Stream(DisplayedStream {
            display_name: gen_ref.name.clone(),
            transaction_stream_ref: gen_ref,
            color: None,
            background_color: None,
            manual_name: None,
            rows: last_times_on_row.len(),
        });

        self.insert_item(new_gen, None);
    }

    pub fn add_stream(&mut self, stream_ref: TransactionStreamRef) {
        if self
            .inner
            .as_transactions_mut()
            .unwrap()
            .get_stream(stream_ref.stream_id)
            .unwrap()
            .transactions_loaded
            .not()
        {
            info!("(Stream)Loading transactions into memory!");
            match self
                .inner
                .as_transactions_mut()
                .unwrap()
                .inner
                .load_stream_into_memory(stream_ref.stream_id)
            {
                Ok(_) => info!(
                    "(Stream {}) Finished loading transactions!",
                    stream_ref.stream_id
                ),
                Err(_) => return,
            }
        }

        let stream = self
            .inner
            .as_transactions()
            .unwrap()
            .get_stream(stream_ref.stream_id)
            .unwrap();
        let mut last_times_on_row = vec![(BigUint::ZERO, BigUint::ZERO)];

        for gen_id in &stream.generators {
            let gen = self
                .inner
                .as_transactions()
                .unwrap()
                .get_generator(*gen_id)
                .unwrap();
            calculate_rows_of_stream(&gen.transactions, &mut last_times_on_row);
        }

        let new_stream = DisplayedItem::Stream(DisplayedStream {
            display_name: stream_ref.name.clone(),
            transaction_stream_ref: stream_ref,
            color: None,
            background_color: None,
            manual_name: None,
            rows: last_times_on_row.len(),
        });

        self.insert_item(new_stream, None);
    }

    pub fn add_all_streams(&mut self) {
        let mut streams: Vec<(usize, String)> = vec![];
        for stream in self.inner.as_transactions().unwrap().get_streams() {
            streams.push((stream.id, stream.name.clone()));
        }

        for (id, name) in streams {
            self.add_stream(TransactionStreamRef::new_stream(id, name));
        }
    }

    /// Insert item after item vidx if Some(vidx).
    /// If None, insert after focused item if there is one, otherwise insert at the end.
    /// Focus on the inserted item if there was a focues item.
    pub(crate) fn insert_item(
        &mut self,
        new_item: DisplayedItem,
        vidx: Option<DisplayedItemIndex>,
    ) {
        if let Some(DisplayedItemIndex(current_idx)) = vidx {
            let insert_idx = current_idx + 1;
            let id = self.next_displayed_item_ref();
            self.displayed_items_order.insert(insert_idx, id);
            self.displayed_items.insert(id, new_item);
            id
        } else if let Some(DisplayedItemIndex(focus_idx)) = self.focused_item {
            let insert_idx = focus_idx + 1;
            let id = self.next_displayed_item_ref();
            self.displayed_items_order.insert(insert_idx, id);
            self.displayed_items.insert(id, new_item);
            self.focused_item = Some(insert_idx.into());
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
        self.displayed_items
            .retain(|_, item| !matches!(item, DisplayedItem::Placeholder(_)));
        let remaining_ids = self.displayed_items_order.clone();
        self.displayed_items_order
            .retain(|i| remaining_ids.contains(i));
    }

    #[inline]
    pub fn any_displayed(&self) -> bool {
        !self.displayed_items.is_empty()
    }

    /// Find the top-most of the currently visible items.
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
            .map_or(default, |(idx, _)| idx)
    }

    /// Find the item at a given y-location.
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
        // only scroll if new location is outside of visible area
        if self.scroll_offset > self.total_height {
            self.scroll_offset = item_y - first_element_y;
        }
    }

    /// Set cursor at next (or previous, if `next` is false) transition of `variable`. If `skip_zero` is true,
    /// use the next transition to a non-zero value.
    pub fn set_cursor_at_transition(
        &mut self,
        next: bool,
        variable: Option<DisplayedItemIndex>,
        skip_zero: bool,
    ) {
        if let Some(DisplayedItemIndex(vidx)) = variable.or(self.focused_item) {
            if let Some(cursor) = &self.cursor {
                if let Some(DisplayedItem::Variable(variable)) = &self
                    .displayed_items_order
                    .get(vidx)
                    .and_then(|id| self.displayed_items.get(id))
                {
                    if let Ok(Some(res)) = self.inner.as_waves().unwrap().query_variable(
                        &variable.variable_ref,
                        &cursor.to_biguint().unwrap_or_default(),
                    ) {
                        if next {
                            if let Some(ref time) = res.next {
                                let stime = time.to_bigint();
                                if stime.is_some() {
                                    self.cursor.clone_from(&stime);
                                }
                            } else {
                                // No next transition, go to end
                                self.cursor = Some(self.num_timestamps().clone());
                            }
                        } else if let Some(stime) = res.current.unwrap().0.to_bigint() {
                            let bigone = BigInt::from(1);
                            // Check if we are on a transition
                            if stime == *cursor && *cursor >= bigone {
                                // If so, subtract cursor position by one
                                if let Ok(Some(newres)) =
                                    self.inner.as_waves().unwrap().query_variable(
                                        &variable.variable_ref,
                                        &(cursor - bigone).to_biguint().unwrap_or_default(),
                                    )
                                {
                                    if let Some(current) = newres.current {
                                        if let Some(newstime) = current.0.to_bigint() {
                                            self.cursor = Some(newstime);
                                        }
                                    }
                                }
                            } else {
                                self.cursor = Some(stime);
                            }
                        }

                        // if zero edges should be skipped
                        if skip_zero {
                            // check if the next transition is 0, if so and requested, go to
                            // next positive transition
                            if let Some(time) = &self.cursor {
                                let next_value = self.inner.as_waves().unwrap().query_variable(
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
                                    self.set_cursor_at_transition(next, Some(vidx.into()), false);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn next_displayed_item_ref(&mut self) -> DisplayedItemRef {
        self.display_item_ref_counter += 1;
        self.display_item_ref_counter.into()
    }

    /// Returns the number of timestamps in the current waves. For now, this adjusts the
    /// number of timestamps as returned by wave sources if they specify 0 timestamps. This is
    /// done to avoid having to consider what happens with the viewport. In the future,
    /// we should probably make this an [`Option<BigInt>`].
    pub fn num_timestamps(&self) -> BigInt {
        self.inner
            .max_timestamp()
            .and_then(|r| if r == BigUint::zero() { None } else { Some(r) })
            .unwrap_or(BigUint::from(1u32))
            .to_bigint()
            .unwrap()
    }
}

fn calculate_rows_of_stream(
    transactions: &Vec<Transaction>,
    last_times_on_row: &mut Vec<(BigUint, BigUint)>,
) {
    for transaction in transactions {
        let mut curr_row = 0;
        let start_time = transaction.get_start_time();
        let end_time = transaction.get_end_time();

        while start_time > last_times_on_row[curr_row].0
            && start_time < last_times_on_row[curr_row].1
        {
            curr_row += 1;
            if last_times_on_row.len() <= curr_row {
                last_times_on_row.push((BigUint::ZERO, BigUint::ZERO));
            }
        }
        last_times_on_row[curr_row] = (start_time, end_time);
    }
}
