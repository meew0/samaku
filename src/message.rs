use iced::widget::pane_grid;

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

    // Messages pertaining to the state of the entire application (GlobalState)
    // For example loading/saving media
    Global(GlobalMessage),

    // Message pertaining to a specific pane (PaneState)
    // Will be dispatched to the currently focused pane.
    // For example changing video display settings, or scrolling the timeline
    Dispatch(PaneMessage),
}

impl Message {
    // Returns a function that maps Some(x) to some message, and None to Message::None
    pub fn map_option<A, F1: FnOnce(A) -> Self>(f1: F1) -> impl FnOnce(Option<A>) -> Self {
        |a_opt| match a_opt {
            Some(a) => f1(a),
            None => Message::None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum GlobalMessage {
    LoadVideo,
    VideoFileSelected(std::path::PathBuf),
    LoadSubtitles,
    SubtitleFileRead(String),
    NextFrame,
    PreviousFrame,
}

#[derive(Debug, Clone)]
pub enum PaneMessage {
    VideoIncrementCounter,
}
