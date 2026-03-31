//! Types and functions to implement undo/redo history.
//!
//! The way history is recorded samaku is that every `iced` message, before being processed in `update`,
//! is processed by `History::record`. This method will create a history entry (`Node`) based on the message
//! and the global state. The primary design goal here is to avoid storing copies of the full `subtitle::File`,
//! or even just `EventTrack`, if at all possible (since it may be gigabytes large), and instead do as much
//! as possible incrementally. This leads to two main scenarios in `record`:
//!
//! (1) the origin state and the changes to be made in `update` are trivially determined by the message.
//! In this case, `record` does all the required processing and finishes the recording process on its own.
//!
//! (2) more complex processing is required to determine how exactly the global state is to be changed.
//! (todo elaborate here)

use crate::message::Message;
use std::rc::Rc;

pub struct History {
    pub last: Rc<Node>,
}

/// An entry in the history, as an intrusive linked list with the entries that precede and follow it in the chain.
pub struct Node {
    lore: Lore,
    data: Vec<Message>,
    discriminant: std::mem::Discriminant<Message>,
    prev: Option<Rc<Node>>,
    next: Option<Rc<Node>>,
    timestamp: std::time::Instant,
}

/// The previous state of some object.
pub enum Lore {
    Root,
}

impl Node {
    pub fn root() -> Self {
        Node {
            lore: Lore::Root,
            data: vec![],
            discriminant: std::mem::discriminant(&Message::None),
            prev: None,
            next: None,
            timestamp: std::time::Instant::now(),
        }
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        History {
            last: Rc::new(Node::root()),
        }
    }

    pub fn append(&mut self, mut node: Box<Node>) {
        node.prev = Some(Rc::clone(&self.last));
        self.last = node.into();
    }

    pub fn last(&self) -> Rc<Node> {
        Rc::clone(&self.last)
    }
}

#[expect(
    clippy::pointer_format,
    reason = "some variants of Message may contain pointers deep inside their nesting hierarchy but this is irrelevant here"
)]
pub fn record(
    message: &Message,
    _global_state: &crate::Samaku,
    last_node: Rc<Node>,
) -> Option<Box<Node>> {
    match message {
        // messages that might eventually be recorded in the history (but this is not yet implemented)
        Message::CreateStyle
        | Message::DeleteStyle(_)
        | Message::SetStyleBold(_, _)
        | Message::SetStyleItalic(_, _)
        | Message::SetStyleUnderline(_, _)
        | Message::SetStyleStrikeOut(_, _)
        | Message::AddEvent
        | Message::DeleteSelectedEvents
        | Message::SetActiveEventText(_)
        | Message::SetActiveEventActor(_)
        | Message::SetActiveEventEffect(_)
        | Message::SetActiveEventStyleIndex(_)
        | Message::SetActiveEventLayerIndex(_)
        | Message::SetActiveEventType(_)
        | Message::SetActiveEventStartTime(_)
        | Message::SetActiveEventDuration(_)
        | Message::SetEventStartTimeAndDuration(_, _, _)
        | Message::TextEditorActionPerformed(_, _)
        | Message::CreateEmptyFilter
        | Message::AssignFilterToSelectedEvents(_)
        | Message::UnassignFilterFromSelectedEvents
        | Message::SetActiveFilterName(_)
        | Message::DeleteFilter(_)
        | Message::AddNode(_)
        | Message::MoveNode(_, _, _)
        | Message::ConnectNodes(_)
        | Message::DisconnectNodes(_, _, _)
        | Message::SetReticules(_)
        | Message::UpdateReticulePosition(_, _) => {
            println!("NYI: history recording for message {message:?}");
            None
        }
        // messages that will never need to be recorded in the history
        Message::None
        | Message::Pane(_, _)
        | Message::FocusedPane(_)
        | Message::Node(_, _)
        | Message::SplitPane(_)
        | Message::ClosePane
        | Message::FocusPane(_)
        | Message::DragPane(_)
        | Message::ResizePane(_)
        | Message::SetPaneType(_, _)
        | Message::SetFocusedPaneType(_)
        | Message::Toast(_)
        | Message::CloseToast(_)
        | Message::SelectVideoFile
        | Message::SelectAudioFile
        | Message::NewSubtitleFile
        | Message::ImportSubtitleFile
        | Message::OpenSubtitleFile
        | Message::SaveSubtitleFile
        | Message::ExportSubtitleFile
        | Message::VideoFileSelected(_)
        | Message::VideoLoaded(_)
        | Message::VideoFrameAvailable(_, _)
        | Message::AudioFileSelected(_)
        | Message::SubtitleFileReadForImport(_)
        | Message::SubtitleFileReadForOpen(_)
        | Message::SubtitleParseError(_)
        | Message::PlaybackStep
        | Message::PlaybackAdvanceFrames(_)
        | Message::PlaybackAdvanceSeconds(_)
        | Message::PlaybackSetPosition(_)
        | Message::TogglePlayback
        | Message::Playing(_)
        | Message::ToggleEventSelection(_)
        | Message::SelectOnlyEvent(_)
        | Message::TrackMotionForNode(_, _) => None,
    }
}
