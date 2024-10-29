use bytes::Bytes;
use camino::Utf8PathBuf;
use derive_more::Debug;
use egui::DroppedFile;
use emath::{Pos2, Vec2};
use ftr_parser::types::Transaction;
use num::BigInt;
use serde::Deserialize;
use std::path::PathBuf;
use surver::Status;

use crate::graphics::{Graphic, GraphicId};
use crate::transaction_container::{
    StreamScopeRef, TransactionContainer, TransactionRef, TransactionStreamRef,
};
use crate::translation::DynTranslator;
use crate::viewport::ViewportStrategy;
use crate::wave_data::ScopeType;
use crate::{
    clock_highlighting::ClockHighlightType,
    config::ArrowKeyBindings,
    displayed_item::{DisplayedFieldRef, DisplayedItemIndex, DisplayedItemRef},
    time::{TimeStringFormatting, TimeUnit},
    variable_name_type::VariableNameType,
    wave_container::{ScopeRef, VariableRef, WaveContainer},
    wave_source::{LoadOptions, OpenMode},
    wellen::LoadSignalsResult,
    MoveDir, ReloadWaveformDialog, VariableNameFilterType, WaveSource,
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

#[derive(Debug, Deserialize)]
pub enum Message {
    SetActiveScope(ScopeType),
    AddVariables(Vec<VariableRef>),
    AddScope(ScopeRef, bool),
    AddCount(char),
    AddStreamOrGenerator(TransactionStreamRef),
    AddStreamOrGeneratorFromName(Option<StreamScopeRef>, String),
    AddAllFromStreamScope(String),
    InvalidateCount,
    RemoveItemByIndex(DisplayedItemIndex),
    RemoveItems(Vec<DisplayedItemRef>),
    FocusItem(DisplayedItemIndex),
    ItemSelectRange(DisplayedItemIndex),
    ToggleItemSelected(Option<DisplayedItemIndex>),
    UnfocusItem,
    RenameItem(Option<DisplayedItemIndex>),
    MoveFocus(MoveDir, CommandCount, bool),
    MoveFocusedItem(MoveDir, CommandCount),
    FocusTransaction(Option<TransactionRef>, Option<Transaction>),
    VerticalScroll(MoveDir, CommandCount),
    ScrollToItem(usize),
    SetScrollOffset(f32),
    VariableFormatChange(DisplayedFieldRef, String),
    ItemSelectionClear,
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
    #[serde(skip)]
    SurferServerStatus(web_time::Instant, String, Status),
    LoadFile(Utf8PathBuf, LoadOptions),
    LoadWaveformFileFromUrl(String, LoadOptions),
    LoadFromData(Vec<u8>, LoadOptions),
    #[cfg(feature = "python")]
    LoadPythonTranslator(Utf8PathBuf),
    /// Load a spade translator using the specified top and the specified state encoded as ron.
    LoadSpadeTranslator {
        top: String,
        #[debug(skip)]
        state: String,
    },
    #[cfg(not(target_arch = "wasm32"))]
    ConnectToCxxrtl(String),
    #[serde(skip)]
    WaveHeaderLoaded(
        web_time::Instant,
        WaveSource,
        LoadOptions,
        #[debug(skip)] HeaderResult,
    ),
    #[serde(skip)]
    WaveBodyLoaded(web_time::Instant, WaveSource, #[debug(skip)] BodyResult),
    #[serde(skip)]
    WavesLoaded(
        WaveSource,
        WaveFormat,
        #[debug(skip)] Box<WaveContainer>,
        LoadOptions,
    ),
    #[serde(skip)]
    SignalsLoaded(web_time::Instant, #[debug(skip)] LoadSignalsResult),
    #[serde(skip)]
    TransactionStreamsLoaded(
        WaveSource,
        WaveFormat,
        #[debug(skip)] TransactionContainer,
        LoadOptions,
    ),
    #[serde(skip)]
    Error(color_eyre::eyre::Error),
    #[serde(skip)]
    TranslatorLoaded(#[debug(skip)] Box<DynTranslator>),
    /// Take note that the specified translator errored on a `translates` call on the
    /// specified variable
    BlacklistTranslator(VariableRef, String),
    ToggleSidePanel,
    ShowCommandPrompt(Option<String>),
    FileDropped(DroppedFile),
    #[serde(skip)]
    FileDownloaded(String, Bytes, LoadOptions),
    ReloadConfig,
    ReloadWaveform(bool),
    /// Suggest reloading the current waveform as the file on disk has changed.
    /// This should first take the user's confirmation before reloading the waveform.
    /// However, there is a configuration setting that the user can overwrite.
    #[serde(skip)]
    SuggestReloadWaveform,
    /// Close the 'reload_waveform' dialog.
    /// The `reload_file` boolean is the return value of the dialog.
    /// If `do_not_show_again` is true, the `reload_file` setting will be persisted.
    #[serde(skip)]
    CloseReloadWaveformDialog {
        reload_file: bool,
        do_not_show_again: bool,
    },
    /// Update the waveform dialog UI with the provided dialog model.
    #[serde(skip)]
    UpdateReloadWaveformDialog(ReloadWaveformDialog),
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
    ToggleEmptyScopes,
    ToggleParametersInScopes,
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
    #[cfg(feature = "python")]
    OpenPythonPluginDialog,
    #[cfg(feature = "python")]
    ReloadPythonPlugin,
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
    MoveTransaction {
        next: bool,
    },
    VariableValueToClipbord(Option<DisplayedItemIndex>),
    InvalidateDrawCommands,
    AddGraphic(GraphicId, Graphic),
    RemoveGraphic(GraphicId),

    /// Variable dragging messages
    VariableDragStarted(DisplayedItemIndex),
    VariableDragTargetChanged(DisplayedItemIndex),
    VariableDragFinished,
    AddDraggedVariables(Vec<VariableRef>),
    /// Unpauses the simulation if the wave source supports this kind of interactivity. Otherwise
    /// does nothing
    UnpauseSimulation,
    /// Pause the simulation if the wave source supports this kind of interactivity. Otherwise
    /// does nothing
    PauseSimulation,
    /// Expand the displayed item into subfields. Levels controls how many layers of subfields
    /// are expanded. 0 unexpands it completely
    ExpandDrawnItem {
        item: DisplayedItemRef,
        levels: usize,
    },

    SetViewportStrategy(ViewportStrategy),
    SetConfigFromString(String),
    AddCharToPrompt(char),

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
