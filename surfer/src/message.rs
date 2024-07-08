use bytes::Bytes;
use camino::Utf8PathBuf;
use derivative::Derivative;
use eframe::egui::DroppedFile;
use emath::{Pos2, Vec2};
use num::BigInt;
use serde::Deserialize;
use std::path::PathBuf;

use surver::Status;

use crate::translation::DynTranslator;
use crate::{
    clock_highlighting::ClockHighlightType,
    config::ArrowKeyBindings,
    displayed_item::{DisplayedFieldRef, DisplayedItemIndex},
    time::{TimeStringFormatting, TimeUnit},
    variable_name_type::VariableNameType,
    wave_container::{ScopeRef, VariableRef, WaveContainer},
    wave_source::{LoadOptions, OpenMode},
    wellen::LoadSignalsResult,
    MoveDir, VariableNameFilterType, WaveSource,
};
use crate::{config::HierarchyStyle, wave_source::WaveFormat};

type CommandCount = usize;

pub enum HeaderResult {
    /// result of locally parsing the header of a waveform file with wellen
    Local(Box<wellen::viewers::HeaderResult>),
    /// result of querying a remote surfer server
    Remote(
        std::sync::Arc<wellen::Hierarchy>,
        wellen::FileFormat,
        String,
    ),
}

pub enum BodyResult {
    /// result of locally parsing the body of a waveform file with wellen
    Local(wellen::viewers::BodyResult),
    /// result of querying a remote surfer server
    Remote(Vec<wellen::Time>, String),
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub enum AsyncJob {
    SaveState,
}

#[derive(Derivative, Deserialize)]
#[derivative(Debug)]
pub enum Message {
    SetActiveScope(ScopeRef),
    AddVariables(Vec<VariableRef>),
    AddScope(ScopeRef),
    AddCount(char),
    InvalidateCount,
    RemoveItem(DisplayedItemIndex, CommandCount),
    FocusItem(DisplayedItemIndex),
    UnfocusItem,
    RenameItem(Option<DisplayedItemIndex>),
    MoveFocus(MoveDir, CommandCount),
    MoveFocusedItem(MoveDir, CommandCount),
    VerticalScroll(MoveDir, CommandCount),
    ScrollToItem(usize),
    SetScrollOffset(f32),
    VariableFormatChange(DisplayedFieldRef, String),
    ItemColorChange(Option<DisplayedItemIndex>, Option<String>),
    ItemBackgroundColorChange(Option<DisplayedItemIndex>, Option<String>),
    ItemNameChange(Option<DisplayedItemIndex>, Option<String>),
    ChangeVariableNameType(Option<DisplayedItemIndex>, VariableNameType),
    ForceVariableNameTypes(VariableNameType),
    SetNameAlignRight(bool),
    SetClockHighlightType(ClockHighlightType),
    // Reset the translator for this variable back to default. Sub-variables,
    // i.e. those with the variable idx and a shared path are also reset
    ResetVariableFormat(DisplayedFieldRef),
    CanvasScroll {
        delta: Vec2,
        viewport_idx: usize,
    },
    CanvasZoom {
        mouse_ptr: Option<BigInt>,
        delta: f32,
        viewport_idx: usize,
    },
    ZoomToRange {
        start: BigInt,
        end: BigInt,
        viewport_idx: usize,
    },
    CursorSet(BigInt),
    RightCursorSet(Option<BigInt>),
    #[serde(skip)]
    SurferServerStatus(web_time::Instant, String, Status),
    LoadWaveformFile(Utf8PathBuf, LoadOptions),
    LoadWaveformFileFromUrl(String, LoadOptions),
    LoadWaveformFileFromData(Vec<u8>, LoadOptions),
    #[cfg(not(target_arch = "wasm32"))]
    ConnectToCxxrtl(String),
    #[serde(skip)]
    WaveHeaderLoaded(
        web_time::Instant,
        WaveSource,
        LoadOptions,
        #[derivative(Debug = "ignore")] HeaderResult,
    ),
    #[serde(skip)]
    WaveBodyLoaded(
        web_time::Instant,
        WaveSource,
        #[derivative(Debug = "ignore")] BodyResult,
    ),
    #[serde(skip)]
    WavesLoaded(
        WaveSource,
        WaveFormat,
        #[derivative(Debug = "ignore")] Box<WaveContainer>,
        LoadOptions,
    ),
    #[serde(skip)]
    SignalsLoaded(
        web_time::Instant,
        #[derivative(Debug = "ignore")] LoadSignalsResult,
    ),
    #[serde(skip)]
    Error(color_eyre::eyre::Error),
    #[serde(skip)]
    TranslatorLoaded(#[derivative(Debug = "ignore")] Box<DynTranslator>),
    /// Take note that the specified translator errored on a `translates` call on the
    /// specified variable
    BlacklistTranslator(VariableRef, String),
    ToggleSidePanel,
    ShowCommandPrompt(bool),
    FileDropped(DroppedFile),
    #[serde(skip)]
    FileDownloaded(String, Bytes, LoadOptions),
    ReloadConfig,
    ReloadWaveform(bool),
    RemovePlaceholders,
    ZoomToFit {
        viewport_idx: usize,
    },
    GoToStart {
        viewport_idx: usize,
    },
    GoToEnd {
        viewport_idx: usize,
    },
    GoToTime(Option<BigInt>, usize),
    ToggleMenu,
    ToggleToolbar,
    ToggleOverview,
    ToggleStatusbar,
    ToggleIndices,
    ToggleDirection,
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
    SaveStateFile(Option<PathBuf>),
    LoadStateFile(Option<PathBuf>),
    LoadState(crate::State, Option<PathBuf>),
    SetStateFile(PathBuf),
    SetAboutVisible(bool),
    SetKeyHelpVisible(bool),
    SetGestureHelpVisible(bool),
    SetQuickStartVisible(bool),
    SetUrlEntryVisible(bool),
    SetLicenseVisible(bool),
    SetRenameItemVisible(bool),
    SetLogsVisible(bool),
    SetDragStart(Option<Pos2>),
    SetFilterFocused(bool),
    SetVariableNameFilterType(VariableNameFilterType),
    SetVariableNameFilterCaseInsensitive(bool),
    SetUIZoomFactor(f32),
    SetPerformanceVisible(bool),
    SetContinuousRedraw(bool),
    SetCursorWindowVisible(bool),
    ToggleFullscreen,
    SetHierarchyStyle(HierarchyStyle),
    SetArrowKeyBindings(ArrowKeyBindings),
    // Second argument is position to insert after, None inserts after focused item,
    // or last if no focused item
    AddDivider(Option<String>, Option<DisplayedItemIndex>),
    // Argument is position to insert after, None inserts after focused item,
    // or last if no focused item
    AddTimeLine(Option<DisplayedItemIndex>),
    ToggleTickLines,
    ToggleVariableTooltip,
    /// Set a marker at a specific position. If it doesn't exist, it will be created
    SetMarker {
        id: u8,
        time: BigInt,
    },
    MoveMarkerToCursor(u8),
    GoToMarkerPosition(u8, usize),
    MoveCursorToTransition {
        next: bool,
        variable: Option<DisplayedItemIndex>,
        skip_zero: bool,
    },
    VariableValueToClipbord(Option<DisplayedItemIndex>),
    InvalidateDrawCommands,

    /// Variable dragging messages
    VariableDragStarted(DisplayedItemIndex),
    VariableDragTargetChanged(DisplayedItemIndex),
    VariableDragFinished,

    /// Unpauses the simulation if the wave source supports this kind of interactivity. Otherwise
    /// does nothing
    UnpauseSimulation,
    /// Pause the simulation if the wave source supports this kind of interactivity. Otherwise
    /// does nothing
    PauseSimulation,

    /// Run more than one message in sequence
    Batch(Vec<Message>),
    AddViewport,
    RemoveViewport,
    /// Select Theme
    SelectTheme(Option<String>),
    /// Undo the last n changes
    Undo(usize),
    /// Redo the last n changes
    Redo(usize),
    /// Exit the application. This has no effect on wasm and closes the window
    /// on other platforms
    Exit,
    AsyncDone(AsyncJob),
}
