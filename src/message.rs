use std::path::PathBuf;

use bytes::Bytes;
use camino::Utf8PathBuf;
use derivative::Derivative;
use eframe::{
    egui::DroppedFile,
    epaint::{Pos2, Vec2},
};
use num::BigInt;
use serde::Deserialize;

use crate::{
    clock_highlighting::ClockHighlightType,
    config::ArrowKeyBindings,
    graphics::{Graphic, GraphicId},
    time::{TimeStringFormatting, TimeUnit},
    translation::Translator,
    variable_name_type::VariableNameType,
    wave_container::{FieldRef, ScopeRef, VariableRef, WaveContainer},
    wave_source::{LoadOptions, OpenMode},
    MoveDir, VariableNameFilterType, WaveSource, viewport::ViewportStrategy,
};
use crate::{config::HierarchyStyle, wave_source::WaveFormat};

type CommandCount = usize;

#[derive(Derivative, Deserialize)]
#[derivative(Debug)]
pub enum Message {
    SetActiveScope(ScopeRef),
    AddVariable(VariableRef),
    AddScope(ScopeRef),
    AddCount(char),
    InvalidateCount,
    RemoveItem(usize, CommandCount),
    FocusItem(usize),
    UnfocusItem,
    RenameItem(Option<usize>),
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
        #[derivative(Debug = "ignore")] wellen::viewers::HeaderResult,
    ),
    #[serde(skip)]
    WaveBodyLoaded(
        web_time::Instant,
        WaveSource,
        #[derivative(Debug = "ignore")] wellen::viewers::BodyResult,
    ),
    #[serde(skip)]
    WavesLoaded(
        WaveSource,
        WaveFormat,
        #[derivative(Debug = "ignore")] Box<WaveContainer>,
        LoadOptions,
    ),
    #[serde(skip)]
    Error(color_eyre::eyre::Error),
    #[serde(skip)]
    TranslatorLoaded(#[derivative(Debug = "ignore")] Box<dyn Translator + Send>),
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
    AddDivider(Option<String>, Option<usize>),
    // Argument is position to insert after, None inserts after focused item,
    // or last if no focused item
    AddTimeLine(Option<usize>),
    ToggleTickLines,
    ToggleVariableTooltip,
    /// Set a marker at a specific position. If it doesn't exist, it will be created
    SetMarker {
        id: u8,
        time: BigInt,
    },
    MoveMarkerToCursor(u8),
    GoToMarkerPosition(u8, usize),
    SaveState(PathBuf),
    MoveCursorToTransition {
        next: bool,
        variable: Option<usize>,
        skip_zero: bool,
    },
    VariableValueToClipbord(Option<usize>),
    InvalidateDrawCommands,
    AddGraphic(GraphicId, Graphic),
    AddTestGraphics,

    /// Unpauses the simulation if the wave source supports this kind of interactivity. Otherwise
    /// does nothing
    UnpauseSimulation,
    /// Pause the simulation if the wave source supports this kind of interactivity. Otherwise
    /// does nothing
    PauseSimulation,

    AddViewport,
    RemoveViewport,
    SetViewportStrategy(ViewportStrategy),


    /// Run more than one message in sequence
    Batch(Vec<Message>),
    /// Exit the application. This has no effect on wasm and closes the window
    /// on other platforms
    Exit,
}
