use iced::widget::pane_grid;

use crate::{media, model, nde, pane, subtitle, view};

#[derive(Debug, Clone)]
pub enum Message {
    /// Empty message. Does nothing.
    /// Useful when you need to return a Message from something,
    /// but don't want anything to happen
    None,

    /// Message pertaining to a specific pane (PaneState)
    /// Will be dispatched to the given pane (`Pane`) or the focused one (`FocusedPane`).
    /// For example changing video display settings, or scrolling the timeline
    Pane(pane_grid::Pane, Pane),
    FocusedPane(Pane),

    /// Message pertaining to a specific node. Will be dispatched to the given node,
    /// if it exists.
    Node(usize, Node),

    // Messages pertaining to the fundamental pane grid UI (Samaku object)
    SplitPane(pane_grid::Axis),
    ClosePane,
    FocusPane(pane_grid::Pane),
    DragPane(pane_grid::DragEvent),
    ResizePane(pane_grid::ResizeEvent),

    /// Set the given pane to contain the given state.
    /// Can be used to change its type or possibly more
    SetPaneState(pane_grid::Pane, Box<pane::State>),

    /// Show a toast notification
    Toast(view::toast::Toast),

    /// Dismiss a toast notification
    CloseToast(usize),

    // Open a dialog to select the respective type of file.
    SelectVideoFile,
    SelectAudioFile,

    /// Import — use libass for parsing the .ass file. This will strip all extra
    /// Aegisub-/samaku-specific data.
    ImportSubtitleFile,

    /// Open — use our own parser for .ass parsing. This will load NDE filters and keep
    /// other metadata intact.
    OpenSubtitleFile,

    /// A video file has been selected and should be loaded.
    VideoFileSelected(std::path::PathBuf),

    /// A video has been loaded; its metadata is now available and frames can now be decode
    /// from it.
    VideoLoaded(Box<media::VideoMetadata>),

    /// A video frame has been decoded and is available to be displayed.
    VideoFrameAvailable(model::FrameNumber, iced::widget::image::Handle),

    /// An audio file has been selected and should be loaded.
    AudioFileSelected(std::path::PathBuf),

    /// A subtitle file has been selected and read, and its contents are now available.
    SubtitleFileReadForImport(String),

    /// The playback position has changed, so there might now be a new frame to decode.
    ///
    /// This message is necessary because we represent the playback state using interior mutability
    /// within `SharedState`, and iced does not otherwise know when that state changes.
    PlaybackStep,

    // Change the playback state in the given way.
    PlaybackAdvanceFrames(model::FrameDelta),
    PlaybackAdvanceSeconds(f64),
    TogglePlayback,

    /// Update the global representation of the playback state; emitted by the playback worker.
    /// Does not cause the playback state itself to change.
    Playing(bool),

    /// Add an empty sline to the end of the track.
    AddSline,

    /// Set the given sline to be active.
    SelectSline(usize),

    /// Set the text of the active sline.
    SetActiveSlineText(String),

    // Create, update, assign, and delete NDE filters.
    CreateEmptyFilter,
    AssignFilterToActiveSline(subtitle::ExtradataId),
    UnassignFilterFromActiveSline,
    SetActiveFilterName(String),
    DeleteFilter(subtitle::ExtradataId), // NYI

    // Create and update nodes in the current NDE filter.
    AddNode(nde::node::Constructor),
    MoveNode(usize, f32, f32),
    ConnectNodes(iced_node_editor::Link),
    DisconnectNodes(
        iced_node_editor::LogicalEndpoint,
        iced::Point,
        pane_grid::Pane,
    ),

    // Create and update reticules — the controls visible on top of the video when triggered by
    // certain NDE nodes.
    SetReticules(model::reticule::Reticules),
    UpdateReticulePosition(usize, nde::tags::Position),

    /// Tell the video playback worker to start motion tracking and sending the results to the
    /// node with the given ID.
    TrackMotionForNode(usize, media::motion::Region),
}

impl Message {
    /// Returns a function that maps Some(x) to some message, and None to Message::None.
    #[must_use]
    pub fn map_option<A, F1: FnOnce(A) -> Self>(f1: F1) -> impl FnOnce(Option<A>) -> Self {
        |a_opt| match a_opt {
            Some(a) => f1(a),
            None => Self::None,
        }
    }

    /// Returns a function that maps Ok(x) to some message, and Err(y) to some other message.
    #[must_use]
    pub fn map_result<A, B, F1: FnOnce(A) -> Self, F2: FnOnce(B) -> Self>(
        f_ok: F1,
        f_err: F2,
    ) -> impl FnOnce(Result<A, B>) -> Self {
        |result| match result {
            Ok(a) => f_ok(a),
            Err(b) => f_err(b),
        }
    }
}

// Utility functions to create toasts
#[must_use]
pub fn toast_danger(title: String, body: String) -> Message {
    Message::Toast(view::toast::Toast {
        title,
        body,
        status: view::toast::Status::Danger,
    })
}

/// Messages dispatched to panes.
#[derive(Debug, Clone)]
pub enum Pane {
    // Messages for the subtitle grid
    GridSyncHeader(iced::widget::scrollable::AbsoluteOffset),
    GridColumnResizing(usize, f32),
    GridColumnResized,

    // Messages for the node editor
    NodeEditorScaleChanged(f32, f32, f32),
    NodeEditorTranslationChanged(f32, f32),
    NodeEditorDangling(Option<(iced_node_editor::LogicalEndpoint, iced_node_editor::Link)>),
    NodeEditorFilterSelected(usize, pane::node_editor::FilterReference),
}

/// Messages dispatched to nodes.
#[derive(Debug, Clone)]
pub enum Node {
    /// A new marker is available for the currently running motion track.
    MotionTrackUpdate(model::FrameNumber, media::motion::Region),

    /// The text input in a node has changed, to be used generically by different nodes.
    TextInputChanged(String),
}
