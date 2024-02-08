use std::path::PathBuf;

use bytes::Bytes;
use camino::Utf8PathBuf;
use derivative::Derivative;
use eframe::{
    egui::DroppedFile,
    epaint::{Pos2, Vec2},
};
use num::BigInt;

use crate::wave_source::WaveFormat;
use crate::{
    clock_highlighting::ClockHighlightType,
    time::{TimeStringFormatting, TimeUnit},
    translation::Translator,
    variable_name_type::VariableNameType,
    wave_container::{FieldRef, ModuleRef, VariableRef, WaveContainer},
    wave_source::{LoadOptions, OpenMode},
    MoveDir, VariableNameFilterType, WaveSource,
};

type CommandCount = usize;

#[derive(Derivative)]
#[derivative(Debug)]
pub enum Message {
    SetActiveScope(ModuleRef),
    AddVariable(VariableRef),
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
    ScrollToItem(usize),
    SetScrollOffset(f32),
    VariableFormatChange(FieldRef, String),
    ItemColorChange(Option<usize>, Option<String>),
    ItemBackgroundColorChange(Option<usize>, Option<String>),
    ItemNameChange(Option<usize>, Option<String>),
    ChangeVariableNameType(Option<usize>, VariableNameType),
    ForceVariableNameTypes(VariableNameType),
    SetNameAlignRight(bool),
    SetClockHighlightType(ClockHighlightType),
    // Reset the translator for this variable back to default. Sub-variables,
    // i.e. those with the variable idx and a shared path are also reset
    ResetVariableFormat(FieldRef),
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
    LoadWaveformFile(Utf8PathBuf, LoadOptions),
    LoadWaveformFileFromUrl(String, LoadOptions),
    LoadWaveformFileFromData(Vec<u8>, LoadOptions),
    WavesLoaded(WaveSource, WaveFormat, Box<WaveContainer>, LoadOptions),
    Error(color_eyre::eyre::Error),
    TranslatorLoaded(#[derivative(Debug = "ignore")] Box<dyn Translator + Send>),
    /// Take note that the specified translator errored on a `translates` call on the
    /// specified variable
    BlacklistTranslator(VariableRef, String),
    ToggleSidePanel,
    ShowCommandPrompt(bool),
    FileDropped(DroppedFile),
    FileDownloaded(String, Bytes, LoadOptions),
    ReloadConfig,
    ReloadWaveform(bool),
    RemovePlaceholders,
    ZoomToFit,
    GoToStart,
    GoToEnd,
    GoToTime(Option<BigInt>),
    ToggleMenu,
    ToggleToolbar,
    ToggleOverview,
    ToggleStatusbar,
    ToggleIndices,
    SetTimeUnit(TimeUnit),
    SetTimeStringFormatting(Option<TimeStringFormatting>),
    CommandPromptClear,
    CommandPromptUpdate {
        suggestions: Vec<(String, Vec<bool>)>,
    },
    CommandPromptPushPrevious(String),
    SelectPrevCommand,
    SelectNextCommand,
    OpenFileDialog(OpenMode),
    OpenSaveStateDialog,
    SetAboutVisible(bool),
    SetKeyHelpVisible(bool),
    SetGestureHelpVisible(bool),
    SetQuickStartVisible(bool),
    SetUrlEntryVisible(bool),
    SetRenameItemVisible(bool),
    SetLogsVisible(bool),
    SetDragStart(Option<Pos2>),
    SetFilterFocused(bool),
    SetVariableNameFilterType(VariableNameFilterType),
    SetUiScale(f32),
    SetPerformanceVisible(bool),
    SetContinuousRedraw(bool),
    SetCursorWindowVisible(bool),
    ToggleFullscreen,
    // Second argument is position to insert after, None inserts after focused item,
    // or last if no focused item
    AddDivider(Option<String>, Option<usize>),
    // Argument is position to insert after, None inserts after focused item,
    // or last if no focused item
    AddTimeLine(Option<usize>),
    ToggleTickLines,
    ToggleVariableTooltip,
    SetCursorPosition(u8),
    GoToCursorPosition(u8),
    SaveState(PathBuf),

    /// Run more than one message in sequence
    Batch(Vec<Message>),
    /// Exit the application. This has no effect on wasm and closes the window
    /// on other platforms
    Exit,
}
