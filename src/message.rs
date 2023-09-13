use iced::widget::pane_grid;

use crate::{media, pane, workers};

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

    /// Message pertaining to a specific worker. Will be dispatched to it
    Worker(WorkerMessage),

    // Messages pertaining to the fundamental pane grid UI (Samaku object)
    SplitPane(pane_grid::Axis),
    ClosePane,
    FocusPane(pane_grid::Pane),
    DragPane(pane_grid::DragEvent),
    ResizePane(pane_grid::ResizeEvent),

    /// Set the given pane to contain the given state.
    /// Can be used to change its type or possibly more
    SetPaneState(pane_grid::Pane, Box<pane::PaneState>),

    /// Spawn a worker.
    /// Guaranteed to be idempotent — does nothing if the specified worker
    /// is already spawned
    SpawnWorker(workers::Type),

    // Open a dialog to select the respective type of file.
    SelectVideoFile,
    SelectAudioFile,
    SelectSubtitleFile,

    VideoLoaded(Box<media::VideoMetadata>),

    VideoFrameAvailable(i32, iced::widget::image::Handle),

    AudioFileSelected(std::path::PathBuf),
    SubtitleFileRead(String),

    PlaybackAdvanceFrames(i32),
    PlaybackAdvanceSeconds(f64),
    TogglePlayback,
}

impl Message {
    // Returns a Command that does nothing but return some message
    pub fn command(message: Self) -> iced::Command<Self> {
        iced::Command::perform(async { message }, |m| m)
    }

    // Returns a Command that returns several messages
    pub fn command_all(messages: impl IntoIterator<Item = Self>) -> iced::Command<Self> {
        iced::Command::batch(messages.into_iter().map(|m| Self::command(m)))
    }

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

#[derive(Debug, Clone)]
pub enum WorkerMessage {
    VideoDecoder(VideoDecoderMessage),
    CpalPlayback(CpalPlaybackMessage),
}

#[derive(Debug, Clone)]
pub enum VideoDecoderMessage {
    PlaybackStep,
    LoadVideo(std::path::PathBuf),
}

#[derive(Debug, Clone)]
pub enum CpalPlaybackMessage {
    TryRestart,

    // These are NOT for playing and pausing video/audio playback
    // at the application level (use global_state.playback_state.playing),
    // but for the stream level
    Play,
    Pause,
}

// Returns all PlaybackStep messages. For now, there's only 1
pub fn playback_step_all() -> Vec<Message> {
    vec![Message::Worker(WorkerMessage::VideoDecoder(
        VideoDecoderMessage::PlaybackStep,
    ))]
}
