use iced::widget::pane_grid;

use crate::{media, pane};

#[derive(Debug, Clone)]
pub enum Message {
    /// Empty message. Does nothing.
    /// Useful when you need to return a Message from something,
    /// but don't want anything to happen
    None,

    /// Message pertaining to a specific pane (PaneState)
    /// Will be dispatched to the currently focused pane.
    /// For example changing video display settings, or scrolling the timeline
    Pane(PaneMessage),

    // Messages pertaining to the fundamental pane grid UI (Samaku object)
    SplitPane(pane_grid::Axis),
    ClosePane,
    FocusPane(pane_grid::Pane),
    DragPane(pane_grid::DragEvent),
    ResizePane(pane_grid::ResizeEvent),

    /// Set the given pane to contain the given state.
    /// Can be used to change its type or possibly more
    SetPaneState(pane_grid::Pane, Box<pane::PaneState>),

    // Open a dialog to select the respective type of file.
    SelectVideoFile,
    SelectAudioFile,
    SelectSubtitleFile,

    VideoFileSelected(std::path::PathBuf),
    VideoLoaded(Box<media::VideoMetadata>),

    VideoFrameAvailable(i32, iced::widget::image::Handle),

    AudioFileSelected(std::path::PathBuf),
    SubtitleFileRead(String),

    /// Notify workers that the playback state changed.
    PlaybackStep,

    PlaybackAdvanceFrames(i32),
    PlaybackAdvanceSeconds(f64),
    TogglePlayback,
}

impl Message {
    // Returns a function that maps Some(x) to some message, and None to Message::None
    pub fn map_option<A, F1: FnOnce(A) -> Self>(f1: F1) -> impl FnOnce(Option<A>) -> Self {
        |a_opt| match a_opt {
            Some(a) => f1(a),
            None => Self::None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PaneMessage {}
