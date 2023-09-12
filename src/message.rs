use iced::widget::pane_grid;

use crate::{controller, media};

#[derive(Debug, Clone)]
pub enum Message {
    // Empty message
    None,

    // Messages pertaining to the fundamental pane grid UI (Samaku object)
    SplitPane(pane_grid::Axis),
    ClosePane,
    FocusPane(pane_grid::Pane),
    DragPane(pane_grid::DragEvent),
    ResizePane(pane_grid::ResizeEvent),
    CyclePaneType,

    // Spawn a worker.
    // Guaranteed to be idempotent — does nothing if the specified worker
    // is already spawned
    SpawnWorker(controller::workers::Type),

    // Messages pertaining to the state of the entire application (GlobalState)
    // For example loading/saving media
    Global(GlobalMessage),

    // Message pertaining to a specific pane (PaneState)
    // Will be dispatched to the currently focused pane.
    // For example changing video display settings, or scrolling the timeline
    Pane(PaneMessage),

    // Message pertaining to a specific worker. Will be dispatched to it
    Worker(WorkerMessage),
}

impl Message {
    // Returns a Command that does nothing but return some message
    pub fn command(message: Self) -> iced::Command<Self> {
        iced::Command::perform(async { message }, |m| m)
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
pub enum GlobalMessage {
    // Open a dialog to select the respective type of file.
    SelectVideoFile,
    SelectAudioFile,
    SelectSubtitleFile,

    VideoLoaded(Box<media::VideoMetadata>),

    AudioFileSelected(std::path::PathBuf),
    SubtitleFileRead(String),
    NextFrame,
    PreviousFrame,
}

#[derive(Debug, Clone)]
pub enum PaneMessage {
    VideoFrameAvailable(i32, iced::widget::image::Handle),
}

#[derive(Debug, Clone)]
pub enum WorkerMessage {
    VideoDecoder(VideoDecoderMessage),
}

#[derive(Debug, Clone)]
pub enum VideoDecoderMessage {
    PlaybackStep,
    LoadVideo(std::path::PathBuf),
}
