use iced::widget::pane_grid;
use std::borrow::Cow;
use std::collections::HashSet;

use crate::{action, media, model, nde, pane, subtitle};

#[derive(Debug, Clone)]
pub enum Message {
    /// Empty message. Does nothing.
    /// Useful when you need to return a Message from something,
    /// but don't want anything to happen.
    None,

    /// Message pertaining to a specific pane (PaneState)
    /// Will be dispatched to the given pane (`Pane`) or the focused one (`FocusedPane`).
    /// For example changing video display settings, or scrolling the timeline.
    Pane(pane_grid::Pane, Pane),
    FocusedPane(Pane),

    /// Message pertaining to a specific node. Will be dispatched to the given node,
    /// if it exists.
    Node(subtitle::ExtradataId, nde::graph::NodeId, Node),

    /// The currently pressed keyboard modifiers (control, shift, etc) have changed.
    ModifiersChanged(iced::keyboard::Modifiers),

    // Messages pertaining to the fundamental pane grid UI (Samaku object)
    SplitPane(pane_grid::Axis),
    ClosePane,
    FocusPane(pane_grid::Pane),
    DragPane(pane_grid::DragEvent),
    ResizePane(pane_grid::ResizeEvent),

    /// Set the type of the given pane based on the given pane constructor.
    SetPaneType(pane_grid::Pane, pane::Constructor),

    /// Same as [`SetPaneType`], but sets the focused pane.
    SetFocusedPaneType(pane::Constructor),

    /// Show a toast notification.
    Toast(model::toast::Toast<Message>),

    /// Dismiss a toast notification.
    CloseToast(usize),

    /// Update the progress value of the progress-bar toast with the given stable ID.
    UpdateToastProgress(model::toast::Id, f32),

    // History control messages
    Undo,
    Redo,

    // Open a dialog to select the respective type of file.
    SelectVideoFile,
    SelectAudioFile,

    /// Clear loaded subtitles and start anew.
    NewSubtitleFile,

    /// Import — use libass for parsing the .ass file. This will strip all extra
    /// Aegisub-/samaku-specific data.
    ImportSubtitleFile,

    /// Open — use our own parser for .ass parsing. This will load NDE filters and keep
    /// other metadata intact.
    OpenSubtitleFile,

    /// Save subtitle file — storing events as they are represented internally, with NDE filters
    /// reproduced intact as extradata.
    SaveSubtitleFile,

    /// Export subtitle file — compiling events and removing extraneous metadata.
    ExportSubtitleFile,

    /// A video file has been selected and should be indexed, then loaded.
    VideoFileSelected(std::path::PathBuf),

    /// A video file has been successfully indexed and should now be loaded.
    /// Uses `NeverClone`, so this message must never be cloned.
    VideoIndexed(std::path::PathBuf, model::NeverClone<media::Index>),

    /// A video has been loaded; its metadata is now available and frames can now be decoded
    /// from it.
    VideoLoaded(Box<media::VideoMetadata>),

    /// A video frame has been decoded and is available to be displayed.
    VideoFrameAvailable(model::FrameNumber, iced::widget::image::Handle),

    /// An audio file has been selected and should be loaded.
    AudioFileSelected(std::path::PathBuf),

    /// A subtitle file has been selected and read, and its contents are now available.
    SubtitleFileReadForImport(String),

    /// A subtitle file has been selected, read, and parsed into an `AssFile`.
    /// This message uses `NeverClone`, so it should never be cloned.
    SubtitleFileReadForOpen(
        model::NeverClone<Box<(subtitle::File, Vec<subtitle::parse::Warning>)>>,
    ),

    SubtitleParseError(model::NeverClone<subtitle::parse::SubtitleParseError>),

    /// The playback position has changed, so there might now be a new frame to decode.
    ///
    /// This message is necessary because we represent the playback state using interior mutability
    /// within `SharedState`, and iced does not otherwise know when that state changes.
    PlaybackStep,

    // Change the playback state in the given way.
    PlaybackAdvanceFrames(model::FrameDelta),
    PlaybackAdvanceSeconds(f64),
    PlaybackSetPosition(subtitle::StartTime),
    TogglePlayback,

    /// Update the global representation of the playback state; emitted by the playback worker.
    /// Does not cause the playback state itself to change.
    Playing(bool),

    CreateStyle,
    DeleteStyle(usize),
    RestoreStyle(usize, subtitle::Style, HashSet<subtitle::EventIndex>),

    // Set various properties of the given style.
    SetStyleName(usize, String),
    SetStyleFontName(usize, String),
    SetStyleFontSize(usize, f64),
    SetStylePrimaryColour(usize, nde::tags::Colour),
    SetStylePrimaryTransparency(usize, nde::tags::Transparency),
    SetStyleSecondaryColour(usize, nde::tags::Colour),
    SetStyleSecondaryTransparency(usize, nde::tags::Transparency),
    SetStyleBorderColour(usize, nde::tags::Colour),
    SetStyleBorderTransparency(usize, nde::tags::Transparency),
    SetStyleShadowColour(usize, nde::tags::Colour),
    SetStyleShadowTransparency(usize, nde::tags::Transparency),
    SetStyleBold(usize, bool),
    SetStyleItalic(usize, bool),
    SetStyleUnderline(usize, bool),
    SetStyleStrikeOut(usize, bool),
    SetStyleScaleX(usize, f64),
    SetStyleScaleY(usize, f64),
    SetStyleSpacing(usize, f64),
    SetStyleAngle(usize, f64),
    SetStyleBlur(usize, f64),
    SetStyleBorderStyle(usize, subtitle::BorderStyle),
    SetStyleBorderWidth(usize, f64),
    SetStyleShadowDistance(usize, f64),
    SetStyleAlignment(usize, nde::tags::Alignment),
    SetStyleMarginLeft(usize, i32),
    SetStyleMarginRight(usize, i32),
    SetStyleMarginVertical(usize, i32),
    SetStyleJustify(usize, subtitle::JustifyMode),

    /// Add an empty event to the end of the track.
    AddEvent,

    DeleteEvents(HashSet<subtitle::EventIndex>),
    DeleteSelectedEvents,

    RestoreEvents(
        Vec<(subtitle::Tombstone, usize, subtitle::Event<'static>)>,
        model::select::EventSelection,
    ),

    /// Select the given event if it is not selected, otherwise deselect it.
    ToggleEventSelection(subtitle::EventIndex),
    GroupSelectEvents(subtitle::EventIndex, subtitle::EventIndex, bool),
    SetEventSelectionSingle(subtitle::EventIndex, bool, Option<subtitle::EventIndex>),
    SelectOnlyEvent(subtitle::EventIndex),
    SetEventSelection(model::select::EventSelection),
    DeselectEvents(HashSet<subtitle::EventIndex>, Option<subtitle::EventIndex>),
    SelectAllEvents,

    // Set various properties of events.
    MultiEditEventText(action::MultiEdit<Cow<'static, str>>),
    MultiEditEventActor(action::MultiEdit<Cow<'static, str>>),
    MultiEditEventEffect(action::MultiEdit<Cow<'static, str>>),
    MultiEditEventStyleIndex(action::MultiEdit<usize>),
    MultiEditEventLayerIndex(action::MultiEdit<i32>),
    MultiEditEventType(action::MultiEdit<subtitle::EventType>),
    MultiEditEventStartTime(action::MultiEdit<subtitle::StartTime>),
    MultiEditEventDuration(action::MultiEdit<subtitle::Duration>),

    // Set various properties of a specific event.
    SetEventStartTimeAndDuration(
        subtitle::EventIndex,
        subtitle::StartTime,
        subtitle::Duration,
    ),

    // Action performed in a subtitle text editor
    // (needs to be handled both globally and locally)
    TextEditorActionPerformed(
        pane_grid::Pane,
        action::MultiEdit<iced::widget::text_editor::Action>,
    ),

    // Create, update, assign, and delete NDE filters.
    CreateEmptyFilter,
    AssignFilterToEvents(subtitle::ExtradataId, HashSet<subtitle::EventIndex>),
    AssignFilterToSelectedEvents(subtitle::ExtradataId),
    UnassignFilterFromEvents(subtitle::ExtradataId, HashSet<subtitle::EventIndex>),
    UnassignFilterFromSelectedEvents(subtitle::ExtradataId),
    MultiAssignFiltersToEvents(
        Vec<Option<subtitle::ExtradataId>>,
        HashSet<subtitle::EventIndex>,
    ),
    SetFilterName(subtitle::ExtradataId, String),
    SetFilterGraph(subtitle::ExtradataId, nde::Graph),
    DeleteFilter(subtitle::ExtradataId),
    RestoreFilter(
        subtitle::ExtradataId,
        nde::Filter,
        Vec<subtitle::EventIndex>,
    ),

    // Create and update nodes in the specified NDE filter.
    AddNode(subtitle::ExtradataId, nde::node::Constructor),
    DeleteNodes(subtitle::ExtradataId, Vec<nde::graph::NodeId>),
    MoveNode(subtitle::ExtradataId, nde::graph::NodeId, iced::Point),
    MoveNodeGroup(subtitle::ExtradataId, Vec<nde::graph::NodeId>, iced::Vector),
    ConnectNodes(
        subtitle::ExtradataId,
        nde::graph::PreviousEndpoint,
        nde::graph::NextEndpoint,
    ),
    DisconnectNodes(
        subtitle::ExtradataId,
        nde::graph::PreviousEndpoint,
        nde::graph::NextEndpoint,
    ),
    SetNodeConnection(
        subtitle::ExtradataId,
        Option<nde::graph::PreviousEndpoint>,
        nde::graph::NextEndpoint,
    ),

    // Create and update reticules — the controls visible on top of the video when triggered by
    // certain NDE nodes.
    SetReticules(model::reticule::Reticules),
    UpdateReticulePosition(model::reticule::Index, nde::tags::Position),

    /// Tell the video playback worker to start motion tracking and sending the results to the
    /// node with the given ID.
    TrackMotionForNode(
        subtitle::ExtradataId,
        nde::graph::NodeId,
        media::motion::Region,
    ),
}

impl Message {
    /// Returns a function that maps Some(x) to some message, and None to Message::None.
    pub fn map_option<A, F1: Fn(A) -> Self>(f1: F1) -> impl Fn(Option<A>) -> Self {
        move |a_opt| match a_opt {
            Some(val) => f1(val),
            None => Self::None,
        }
    }

    /// Returns a function that maps Ok(x) to some message, and Err(y) to some other message.
    pub fn map_result<A, B, F1: FnOnce(A) -> Self, F2: FnOnce(B) -> Self>(
        f_ok: F1,
        f_err: F2,
    ) -> impl FnOnce(Result<A, B>) -> Self {
        |result| match result {
            Ok(ok_val) => f_ok(ok_val),
            Err(err_val) => f_err(err_val),
        }
    }

    /// Returns a function that takes an `anyhow::Result`, maps the Ok value to some message, and the Err value to a message opening an error toast.
    pub fn map_anyhow<A, F1: Fn(A) -> Self>(f_ok: F1) -> impl Fn(anyhow::Result<A>) -> Self {
        move |result| match result {
            Ok(ok_val) => f_ok(ok_val),
            Err(err_val) => toast_danger("Error".to_owned(), format!("{err_val:#}")),
        }
    }

    /// Returns a function that takes an `anyhow::Result<Option<A>>` and maps:
    ///  - `Ok(Some(...))` to a given message
    ///  - `Ok(None)` to `message::None`
    ///  - `Err(...)` to an error toast message
    pub fn map_anyhow_option<A, F1: Fn(A) -> Self>(
        f_ok: F1,
    ) -> impl Fn(anyhow::Result<Option<A>>) -> Self {
        move |result| match result {
            Ok(Some(ok_val)) => f_ok(ok_val),
            Ok(None) => Message::None,
            Err(err_val) => toast_danger("Error".to_owned(), format!("{err_val:#}")),
        }
    }

    /// Returns the internal name of the message variant.
    pub fn name(&self) -> String {
        #[expect(
            clippy::pointer_format,
            reason = "even if pointer formatting was a problem for samaku, we don't actually use the data part of the result"
        )]
        let debug = format!("{self:?}");

        if let Some(pos) = debug.find('(') {
            debug[..pos].to_owned()
        } else {
            debug
        }
    }
}

// Utility functions to create toasts
#[must_use]
pub fn toast_danger(title: String, body: String) -> Message {
    Message::Toast(model::toast::Toast::message(
        model::toast::Status::Danger,
        title,
        body,
    ))
}

/// Messages dispatched to panes.
#[derive(Debug, Clone)]
pub enum Pane {
    // Messages for the subtitle grid
    GridScroll(iced::widget::scrollable::Viewport),

    // Messages for the node editor
    NodeEditorCameraChanged(iced::Point, f32),
    NodeEditorSelectionChanged(Vec<nde::graph::NodeId>),
    NodeEditorFilterSelected(usize, pane::node_editor::FilterReference),

    // Messages for the style editor
    StyleEditorStyleSelected(usize),

    // Messages for the timeline
    TimelineDragged(pane::timeline::Position),
}

/// Messages dispatched to nodes.
#[derive(Debug, Clone)]
pub enum Node {
    /// A new marker is available for the currently running motion track.
    MotionTrackUpdate(model::FrameNumber, media::motion::Region),

    /// The text input in a node has changed, to be used generically by different nodes.
    TextInputChanged(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name() {
        let message = Message::Playing(true);
        assert_eq!(message.name(), "Playing");
    }
}
