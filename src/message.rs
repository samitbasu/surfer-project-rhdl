use bytes::Bytes;
use camino::Utf8PathBuf;
use derivative::Derivative;
use eframe::{
    egui::DroppedFile,
    epaint::{Pos2, Vec2},
};
use fastwave_backend::Timescale;
use num::BigInt;

use crate::{
    clock_highlighting::ClockHighlightType,
    signal_name_type::SignalNameType,
    translation::Translator,
    wave_container::{FieldRef, ModuleRef, SignalRef, WaveContainer},
    wave_source::OpenMode,
    MoveDir, SignalFilterType, WaveSource,
};

type CommandCount = usize;

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
    UnfocusItem,
    RenameItem(usize),
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
        suggestions: Vec<(String, Vec<bool>)>,
    },
    SelectPrevCommand,
    SelectNextCommand,
    OpenFileDialog(OpenMode),
    SetAboutVisible(bool),
    SetKeyHelpVisible(bool),
    SetGestureHelpVisible(bool),
    SetQuickStartVisible(bool),
    SetUrlEntryVisible(bool),
    SetRenameItemVisible(bool),
    SetLogsVisible(bool),
    SetDragStart(Option<Pos2>),
    SetFilterFocused(bool),
    SetSignalFilterType(SignalFilterType),
    SetUiScale(f32),
    ToggleFullscreen,
    AddDivider(String),
    SetCursorPosition(u8),
    GoToCursorPosition(u8),
    /// Exit the application. This has no effect on wasm and closes the window
    /// on other platforms
    Exit,
}
