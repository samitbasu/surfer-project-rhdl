use std::collections::HashMap;

use bytes::{Buf, Bytes};
use camino::Utf8PathBuf;
use color_eyre::eyre::WrapErr;
use derivative::Derivative;
use eframe::{
    egui::DroppedFile,
    epaint::{Pos2, Vec2},
};
use fastwave_backend::Timescale;
use log::{error, info, trace, warn};
use num::{bigint::ToBigInt, BigInt, FromPrimitive, ToPrimitive};

use crate::{
    clock_highlighting::ClockHighlightType,
    config::SurferConfig,
    displayed_item::DisplayedCursor,
    displayed_item::DisplayedDivider,
    displayed_item::DisplayedItem,
    files::OpenMode,
    signal_name_type::SignalNameType,
    translation::Translator,
    viewport::Viewport,
    wave_container::{FieldRef, ModuleRef, SignalRef, WaveContainer},
    CommandCount, MoveDir, SignalFilterType, State, WaveData, WaveSource,
};

#[derive(Derivative)]
#[derivative(Debug)]
pub enum Message {
    SetActiveScope(ModuleRef),
    AddSignal(SignalRef),
    AddModule(ModuleRef),
    AddCount(char),
    InvalidateCount,
    RemoveItem(usize, CommandCount),
    FocusItem(usize),
    RenameItem(usize),
    UnfocusItem,
    MoveFocus(MoveDir, CommandCount),
    MoveFocusedItem(MoveDir, CommandCount),
    VerticalScroll(MoveDir, CommandCount),
    SetVerticalScroll(usize),
    SignalFormatChange(FieldRef, String),
    ItemColorChange(Option<usize>, Option<String>),
    ItemBackgroundColorChange(Option<usize>, Option<String>),
    ItemNameChange(Option<usize>, String),
    ChangeSignalNameType(Option<usize>, SignalNameType),
    ForceSignalNameTypes(SignalNameType),
    SetClockHighlightType(ClockHighlightType),
    // Reset the translator for this signal back to default. Sub-signals,
    // i.e. those with the signal idx and a shared path are also reset
    ResetSignalFormat(FieldRef),
    CanvasScroll {
        delta: Vec2,
    },
    CanvasZoom {
        mouse_ptr_timestamp: Option<f64>,
        delta: f32,
    },
    ZoomToRange {
        start: f64,
        end: f64,
    },
    CursorSet(BigInt),
    LoadVcd(Utf8PathBuf),
    LoadVcdFromUrl(String),
    WavesLoaded(WaveSource, Box<WaveContainer>, bool),
    Error(color_eyre::eyre::Error),
    TranslatorLoaded(#[derivative(Debug = "ignore")] Box<dyn Translator + Send>),
    /// Take note that the specified translator errored on a `translates` call on the
    /// specified signal
    BlacklistTranslator(SignalRef, String),
    ToggleSidePanel,
    ShowCommandPrompt(bool),
    FileDropped(DroppedFile),
    FileDownloaded(String, Bytes, bool),
    ReloadConfig,
    ReloadWaveform,
    ZoomToFit,
    GoToStart,
    GoToEnd,
    ToggleMenu,
    SetTimeScale(Timescale),
    CommandPromptClear,
    CommandPromptUpdate {
        expanded: String,
        suggestions: Vec<(String, Vec<bool>)>,
    },
    OpenFileDialog(OpenMode),
    SetAboutVisible(bool),
    SetKeyHelpVisible(bool),
    SetGestureHelpVisible(bool),
    SetUrlEntryVisible(bool),
    SetRenameItemVisible(bool),
    SetDragStart(Option<Pos2>),
    SetFilterFocused(bool),
    SetSignalFilterType(SignalFilterType),
    ToggleFullscreen,
    AddDivider(String),
    SetCursorPosition(u8),
    GoToCursorPosition(u8),
    /// Exit the application. This has no effect on wasm and closes the window
    /// on other platforms
    Exit,
}

impl State {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::SetActiveScope(module) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                if waves.inner.has_module(&module) {
                    waves.active_module = Some(module)
                } else {
                    warn!("Setting active scope to {module} which does not exist")
                }
            }
            Message::AddSignal(sig) => {
                self.invalidate_draw_commands();
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                waves.add_signal(&self.translators, &sig)
            }
            Message::AddDivider(name) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                waves
                    .displayed_items
                    .push(DisplayedItem::Divider(DisplayedDivider {
                        color: None,
                        background_color: None,
                        name,
                    }));
            }
            Message::AddModule(module) => {
                let Some(waves) = self.waves.as_mut() else {
                    warn!("Adding module without waves loaded");
                    return;
                };

                let signals = waves.inner.signals_in_module(&module);
                for signal in signals {
                    waves.add_signal(&self.translators, &signal);
                }
                self.invalidate_draw_commands();
            }
            Message::AddCount(digit) => {
                if let Some(count) = &mut self.count {
                    count.push(digit);
                } else {
                    self.count = Some(digit.to_string())
                }
            }
            Message::InvalidateCount => self.count = None,
            Message::FocusItem(idx) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                let visible_signals_len = waves.displayed_items.len();
                if visible_signals_len > 0 && idx < visible_signals_len {
                    waves.focused_item = Some(idx);
                } else {
                    error!(
                        "Can not focus signal {idx} because only {visible_signals_len} signals are visible.",
                    );
                }
            }
            Message::UnfocusItem => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                waves.focused_item = None;
            }
            Message::RenameItem(vidx) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                self.rename_target = Some(vidx);
                *self.item_renaming_string.borrow_mut() =
                    waves.displayed_items.get(vidx).unwrap().name();
            }
            Message::MoveFocus(direction, count) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                let visible_signals_len = waves.displayed_items.len();
                if visible_signals_len > 0 {
                    self.count = None;
                    match direction {
                        MoveDir::Up => {
                            waves.focused_item = waves
                                .focused_item
                                .map_or(Some(visible_signals_len - 1), |focused| {
                                    Some(focused - count.clamp(0, focused))
                                })
                        }
                        MoveDir::Down => {
                            waves.focused_item = waves.focused_item.map_or(
                                Some(waves.scroll + (count - 1).clamp(0, visible_signals_len - 1)),
                                |focused| Some((focused + count).clamp(0, visible_signals_len - 1)),
                            );
                        }
                    }
                }
            }
            Message::SetVerticalScroll(position) => {
                if let Some(waves) = &mut self.waves {
                    waves.scroll = position.clamp(0, waves.displayed_items.len() - 1);
                }
            }
            Message::VerticalScroll(direction, count) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                match direction {
                    MoveDir::Down => {
                        if waves.scroll + count < waves.displayed_items.len() {
                            waves.scroll += count;
                        } else {
                            waves.scroll = waves.displayed_items.len() - 1;
                        }
                    }
                    MoveDir::Up => {
                        if waves.scroll > count {
                            waves.scroll -= count;
                        } else {
                            waves.scroll = 0;
                        }
                    }
                }
            }
            Message::RemoveItem(idx, count) => {
                self.invalidate_draw_commands();

                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                for _ in 0..count {
                    let visible_signals_len = waves.displayed_items.len();
                    if let Some(DisplayedItem::Cursor(cursor)) = waves.displayed_items.get(idx) {
                        waves.cursors.remove(&cursor.idx);
                    }
                    if visible_signals_len > 0 && idx <= (visible_signals_len - 1) {
                        waves.displayed_items.remove(idx);
                        if let Some(focused) = waves.focused_item {
                            if focused == idx {
                                if (idx > 0) && (idx == (visible_signals_len - 1)) {
                                    // if the end of list is selected
                                    waves.focused_item = Some(idx - 1);
                                }
                            } else {
                                if idx < focused {
                                    waves.focused_item = Some(focused - 1)
                                }
                            }
                            if waves.displayed_items.is_empty() {
                                waves.focused_item = None;
                            }
                        }
                    }
                }
                waves.compute_signal_display_names();
            }
            Message::MoveFocusedItem(direction, count) => {
                self.invalidate_draw_commands();
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                if let Some(idx) = waves.focused_item {
                    let visible_signals_len = waves.displayed_items.len();
                    if visible_signals_len > 0 {
                        match direction {
                            MoveDir::Up => {
                                for i in (idx
                                    .saturating_sub(count - 1)
                                    .clamp(1, visible_signals_len - 1)
                                    ..=idx)
                                    .rev()
                                {
                                    waves.displayed_items.swap(i, i - 1);
                                    waves.focused_item = Some(i - 1);
                                }
                            }
                            MoveDir::Down => {
                                for i in idx..(idx + count).clamp(0, visible_signals_len - 1) {
                                    waves.displayed_items.swap(i, i + 1);
                                    waves.focused_item = Some(i + 1);
                                }
                            }
                        }
                    }
                }
            }
            Message::CanvasScroll { delta } => {
                self.invalidate_draw_commands();
                self.handle_canvas_scroll(delta);
            }
            Message::CanvasZoom {
                delta,
                mouse_ptr_timestamp,
            } => {
                self.invalidate_draw_commands();
                self.waves
                    .as_mut()
                    .map(|waves| waves.handle_canvas_zoom(mouse_ptr_timestamp, delta as f64));
            }
            Message::ZoomToFit => {
                self.invalidate_draw_commands();
                self.zoom_to_fit();
            }
            Message::GoToEnd => {
                self.invalidate_draw_commands();
                self.go_to_end();
            }
            Message::GoToStart => {
                self.invalidate_draw_commands();
                self.go_to_start();
            }
            Message::SetTimeScale(timescale) => {
                self.invalidate_draw_commands();
                self.wanted_timescale = timescale;
            }
            Message::ZoomToRange { start, end } => {
                if let Some(waves) = &mut self.waves {
                    waves.viewport.curr_left = start;
                    waves.viewport.curr_right = end;
                }
                self.invalidate_draw_commands();
            }
            Message::SignalFormatChange(field, format) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if self.translators.all_translator_names().contains(&&format) {
                    *waves.signal_format.entry(field.clone()).or_default() = format;

                    if field.field.is_empty() {
                        let Ok(meta) = waves
                            .inner
                            .signal_meta(&field.root)
                            .map_err(|e| warn!("{e:#?}"))
                        else {
                            return;
                        };
                        let translator = waves.signal_translator(&field, &self.translators);
                        let new_info = translator.signal_info(&meta).unwrap();

                        for item in &mut waves.displayed_items {
                            match item {
                                DisplayedItem::Signal(disp) => {
                                    if &disp.signal_ref == &field.root {
                                        disp.info = new_info;
                                        break;
                                    }
                                }
                                DisplayedItem::Cursor(_) => {}
                                DisplayedItem::Divider(_) => {}
                            }
                        }
                    }
                    self.invalidate_draw_commands();
                } else {
                    warn!("No translator {format}")
                }
            }
            Message::ItemColorChange(vidx, color_name) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if let Some(idx) = vidx.or(waves.focused_item) {
                    waves.displayed_items[idx].set_color(color_name);
                };
            }
            Message::ItemNameChange(vidx, name) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if let Some(idx) = vidx.or(waves.focused_item) {
                    waves.displayed_items[idx].set_name(name);
                };
            }
            Message::ItemBackgroundColorChange(vidx, color_name) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };

                if let Some(idx) = vidx.or(waves.focused_item) {
                    waves.displayed_items[idx].set_background_color(color_name)
                };
            }
            Message::ResetSignalFormat(idx) => {
                self.invalidate_draw_commands();
                self.waves
                    .as_mut()
                    .map(|waves| waves.signal_format.remove(&idx));
            }
            Message::CursorSet(new) => {
                if let Some(waves) = self.waves.as_mut() {
                    waves.cursor = Some(new)
                }
            }
            Message::LoadVcd(filename) => {
                self.load_vcd_from_file(filename, false).ok();
            }
            Message::LoadVcdFromUrl(url) => {
                self.load_vcd_from_url(url, false);
            }
            Message::FileDropped(dropped_file) => {
                self.load_vcd_from_dropped(dropped_file, false)
                    .map_err(|e| error!("{e:#?}"))
                    .ok();
            }
            Message::WavesLoaded(filename, new_waves, keep_signals) => {
                info!("VCD file loaded");
                let num_timestamps = new_waves
                    .max_timestamp()
                    .as_ref()
                    .map(|t| t.to_bigint().unwrap())
                    .unwrap_or(BigInt::from_u32(1).unwrap());
                let viewport = Viewport::new(0., num_timestamps.clone().to_f64().unwrap());

                let new_wave = if keep_signals && self.waves.is_some() {
                    self.waves.take().unwrap().update_with(
                        new_waves,
                        filename,
                        num_timestamps,
                        viewport,
                        &self.translators,
                    )
                } else {
                    WaveData {
                        inner: *new_waves,
                        source: filename,
                        active_module: None,
                        displayed_items: vec![],
                        viewport,
                        signal_format: HashMap::new(),
                        num_timestamps,
                        cursor: None,
                        cursors: HashMap::new(),
                        focused_item: None,
                        default_signal_name_type: self.config.default_signal_name_type,
                        scroll: 0,
                    }
                };
                self.invalidate_draw_commands();

                // Must clone timescale before consuming new_vcd
                self.wanted_timescale = new_wave.inner.metadata().timescale.1;
                self.waves = Some(new_wave);
                self.vcd_progress = None;
                info!("Done setting up VCD file");
            }
            Message::BlacklistTranslator(idx, translator) => {
                self.blacklisted_translators.insert((idx, translator));
            }
            Message::Error(e) => {
                error!("{e:?}")
            }
            Message::TranslatorLoaded(t) => {
                info!("Translator {} loaded", t.name());
                self.translators.add(t)
            }
            Message::ToggleSidePanel => {
                self.config.layout.show_hierarchy = !self.config.layout.show_hierarchy;
            }
            Message::ToggleMenu => self.config.layout.show_menu = !self.config.layout.show_menu,
            Message::ShowCommandPrompt(new_visibility) => {
                if !new_visibility {
                    *self.command_prompt_text.borrow_mut() = "".to_string();
                    self.command_prompt.suggestions = vec![];
                    self.command_prompt.expanded = "".to_string();
                }
                self.command_prompt.visible = new_visibility;
            }
            Message::FileDownloaded(url, bytes, keep_signals) => {
                let size = bytes.len() as u64;
                self.load_vcd(
                    WaveSource::Url(url),
                    bytes.reader(),
                    Some(size),
                    keep_signals,
                )
            }
            Message::ReloadConfig => {
                // FIXME think about a structured way to collect errors
                if let Ok(config) =
                    SurferConfig::new().with_context(|| "Failed to load config file")
                {
                    self.config = config;
                    if let Some(ctx) = &self.context {
                        ctx.set_visuals(self.get_visuals())
                    }
                }
            }
            Message::ReloadWaveform => {
                let Some(waves) = &self.waves else { return };
                match &waves.source {
                    WaveSource::File(filename) => {
                        self.load_vcd_from_file(filename.clone(), true).ok()
                    }
                    WaveSource::DragAndDrop(filename) => filename
                        .clone()
                        .and_then(|filename| self.load_vcd_from_file(filename, true).ok()),
                    WaveSource::Url(url) => {
                        self.load_vcd_from_url(url.clone(), true);
                        Some(())
                    }
                };
            }
            Message::SetClockHighlightType(new_type) => {
                self.config.default_clock_highlight_type = new_type
            }
            Message::SetCursorPosition(idx) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                let Some(location) = &waves.cursor else {
                    return;
                };
                if waves
                    .displayed_items
                    .iter()
                    .find_map(|item| match item {
                        DisplayedItem::Cursor(cursor) => {
                            if cursor.idx == idx {
                                Some(cursor)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .is_none()
                {
                    let cursor = DisplayedCursor {
                        color: None,
                        background_color: None,
                        name: format!("Cursor"),
                        idx,
                    };
                    waves.displayed_items.push(DisplayedItem::Cursor(cursor));
                }
                waves.cursors.insert(idx, location.clone());
            }
            Message::GoToCursorPosition(idx) => {
                let Some(waves) = self.waves.as_ref() else {
                    return;
                };

                if let Some(cursor) = waves.cursors.get(&idx) {
                    self.go_to_time(&cursor.clone());
                    self.invalidate_draw_commands();
                }
            }
            Message::ChangeSignalNameType(vidx, name_type) => {
                let Some(waves) = self.waves.as_mut() else {
                    return;
                };
                // checks if vidx is Some then use that, else try focused signal
                if let Some(idx) = vidx.or(waves.focused_item) {
                    if waves.displayed_items.len() > idx {
                        if let DisplayedItem::Signal(signal) = &mut waves.displayed_items[idx] {
                            signal.display_name_type = name_type;
                            waves.compute_signal_display_names();
                        }
                    }
                }
            }
            Message::ForceSignalNameTypes(name_type) => {
                let Some(vcd) = self.waves.as_mut() else {
                    return;
                };
                for signal in &mut vcd.displayed_items {
                    if let DisplayedItem::Signal(signal) = signal {
                        signal.display_name_type = name_type;
                    }
                }
                vcd.default_signal_name_type = name_type;
                vcd.compute_signal_display_names();
            }
            Message::CommandPromptClear => {
                *self.command_prompt_text.borrow_mut() = "".to_string();
                self.command_prompt.expanded = "".to_string();
                self.command_prompt.suggestions = vec![];
            }
            Message::CommandPromptUpdate {
                expanded,
                suggestions,
            } => {
                self.command_prompt.expanded = expanded;
                self.command_prompt.suggestions = suggestions;
            }
            Message::OpenFileDialog(mode) => {
                self.open_file_dialog(mode);
            }
            Message::SetAboutVisible(s) => self.show_about = s,
            Message::SetKeyHelpVisible(s) => self.show_keys = s,
            Message::SetGestureHelpVisible(s) => self.show_gestures = s,
            Message::SetUrlEntryVisible(s) => self.show_url_entry = s,
            Message::SetRenameItemVisible(_) => self.rename_target = None,
            Message::SetDragStart(pos) => self.gesture_start_location = pos,
            Message::SetFilterFocused(s) => self.signal_filter_focused = s,
            Message::SetSignalFilterType(signal_filter_type) => {
                self.signal_filter_type = signal_filter_type
            }
            Message::Exit | Message::ToggleFullscreen => {} // Handled in eframe::update
        }
    }

    pub fn handle_async_messages(&mut self) {
        let mut msgs = vec![];
        loop {
            match self.msg_receiver.try_recv() {
                Ok(msg) => msgs.push(msg),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    trace!("Message sender disconnected");
                    break;
                }
            }
        }

        while let Some(msg) = msgs.pop() {
            self.update(msg);
        }
    }
}
