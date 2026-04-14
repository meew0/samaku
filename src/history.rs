//! Types and functions to implement undo/redo history.
//!
//! The way history is recorded samaku is that every `iced` message, before being processed in `update`,
//! is processed by `History::make_key` and `History::record`.
//! These methods will create a history entry (`Node`) based on the message
//! and the global state. The primary design goal here is to avoid storing copies of the full `subtitle::File`,
//! or even just `EventTrack`, if at all possible (since it may be gigabytes large), and instead do as much
//! as possible incrementally.

use crate::message::Message;
use std::cell::RefCell;
use std::rc::Rc;

pub struct History {
    last: Rc<RefCell<Node>>,
}

/// An entry in the history, as an intrusive linked list with the entries that precede and follow it in the chain.
pub struct Node {
    undo: Vec<Message>,
    redo: Vec<Message>,
    discriminant: std::mem::Discriminant<Message>,
    prev: Option<Rc<RefCell<Node>>>,
    next: Option<Rc<RefCell<Node>>>,
    timestamp: std::time::Instant,
}

impl Node {
    fn root() -> Self {
        Node {
            undo: vec![],
            redo: vec![],
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
    #[must_use]
    pub fn new() -> Self {
        History {
            last: Rc::new(RefCell::new(Node::root())),
        }
    }

    fn last(&self) -> Rc<RefCell<Node>> {
        Rc::clone(&self.last)
    }

    fn make_leaf(&self, message: Message) -> Node {
        let discriminant = std::mem::discriminant(&message);
        Node {
            undo: vec![],
            redo: vec![message],
            discriminant,
            prev: Some(self.last()),
            next: None,
            timestamp: std::time::Instant::now(),
        }
    }

    /// Create a history key based on a reference to a message.
    /// Essentially, this method determines whether the message could sensibly be recorded
    /// in the history, and if so, clones it and creates a suitable `Key`.
    /// Otherwise, it will create a key that panics whenever something tries to put an undo message.
    #[expect(clippy::too_many_lines, reason = "we need to match all messages here")]
    pub fn make_key(&mut self, message: &Message) -> Key {
        match message {
            // messages that might eventually be recorded in the history (but this is not yet implemented)
            Message::CreateStyle
            | Message::DeleteStyle(_)
            | Message::SetStyleName(_, _)
            | Message::SetStyleFontName(_, _)
            | Message::SetStyleFontSize(_, _)
            | Message::SetStylePrimaryColour(_, _)
            | Message::SetStylePrimaryTransparency(_, _)
            | Message::SetStyleSecondaryColour(_, _)
            | Message::SetStyleSecondaryTransparency(_, _)
            | Message::SetStyleBorderColour(_, _)
            | Message::SetStyleBorderTransparency(_, _)
            | Message::SetStyleShadowColour(_, _)
            | Message::SetStyleShadowTransparency(_, _)
            | Message::SetStyleBold(_, _)
            | Message::SetStyleItalic(_, _)
            | Message::SetStyleUnderline(_, _)
            | Message::SetStyleStrikeOut(_, _)
            | Message::SetStyleScaleX(_, _)
            | Message::SetStyleScaleY(_, _)
            | Message::SetStyleSpacing(_, _)
            | Message::SetStyleAngle(_, _)
            | Message::SetStyleBlur(_, _)
            | Message::SetStyleBorderStyle(_, _)
            | Message::SetStyleBorderWidth(_, _)
            | Message::SetStyleShadowDistance(_, _)
            | Message::SetStyleAlignment(_, _)
            | Message::SetStyleMarginLeft(_, _)
            | Message::SetStyleMarginRight(_, _)
            | Message::SetStyleMarginVertical(_, _)
            | Message::SetStyleJustify(_, _)
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
            | Message::DeleteNodes(_)
            | Message::MoveNode(_, _)
            | Message::MoveNodeGroup(_, _)
            | Message::ConnectNodes(_, _)
            | Message::DisconnectNodes(_, _)
            | Message::SetReticules(_)
            | Message::UpdateReticulePosition(_, _) => {
                let cloned = message.clone();
                let node = self.make_leaf(cloned);
                Key::Record(node, None)
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
            | Message::Undo
            | Message::Redo
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
            | Message::TrackMotionForNode(_, _)
            | Message::ModifiersChanged(_)
            | Message::UpdateToastProgress(_, _)
            | Message::VideoIndexed(_, _) => Key::Fail,
        }
    }

    /// Record a previously obtained key (using `make_key`) into the history.
    ///
    /// # Panics
    /// Panics if the key does not follow the leaf node, i.e. if the history has changed between `make_key` and `record`.
    /// Also panics if there is no batch mode specified even though messages have been appended.
    pub fn record(&mut self, key: Key) {
        if let Key::Record(node, batch_mode) = key {
            if node.undo.is_empty() {
                // No data was put into this node
                // (almost certainly because undo/redo is NYI for this particular message)
                return;
            }

            let batch_mode = batch_mode
                .expect("A batch mode should be specified if messages have been appended");

            let prev = node.prev.as_ref().expect("tried to record unlinked node");
            assert!(
                Rc::ptr_eq(prev, &self.last),
                "tried to record node not created from history leaf"
            );

            // Batch the last two nodes together if batching is allowed,
            // if they contain the same redo message
            // and less than five seconds have passed.
            if let BatchMode::Batched {
                undo: undo_append_mode,
                redo: redo_append_mode,
            } = batch_mode
                && {
                    let last = self.last.borrow();
                    let time_delay = node.timestamp - last.timestamp;
                    let same_message = last.discriminant == node.discriminant;
                    same_message && time_delay < std::time::Duration::from_secs(5)
                }
            {
                // Add the new messages into the leaf node
                let Node {
                    undo: mut new_undo,
                    redo: mut new_redo,
                    ..
                } = node;

                match undo_append_mode {
                    BatchAppendMode::Instant => {
                        assert!(!self.last.borrow().undo.is_empty());
                        // No-op. Since the last node already contains the message
                        // needed to undo both the previous and the current message,
                        // we don't need to do anything.
                    }
                    BatchAppendMode::Incremental => {
                        self.last.borrow_mut().undo.append(&mut new_undo);
                    }
                }

                match redo_append_mode {
                    BatchAppendMode::Instant => {
                        // While the undo vec doesn't need to be changed in Instant mode,
                        // the redo vec does, since to redo the whole node, it needs to be
                        // returned to the state after the current node, so we need to store
                        // that state (but nothing else).
                        let last_redo = &mut self.last.borrow_mut().redo;
                        last_redo.clear();
                        last_redo.append(&mut new_redo);
                    }
                    BatchAppendMode::Incremental => {
                        self.last.borrow_mut().redo.append(&mut new_redo);
                    }
                }

                // We intentionally don't update the timestamp here,
                // so that even with constant edits,
                // 5-second undo “steps” are automatically created.
            } else {
                // Append the new node to the linked list
                let rc = Rc::new(RefCell::new(node));
                self.last.borrow_mut().next = Some(Rc::clone(&rc));
                self.last = rc;
            }
        }
    }

    pub fn undo(&mut self) -> Vec<Message> {
        let last = self.last.borrow();
        if let Some(prev) = &last.prev {
            let undo_messages = last.undo.clone();
            let new_last = Rc::clone(prev);
            drop(last);
            self.last = new_last; // step back
            undo_messages
        } else {
            // we reached the root node. Do nothing
            vec![]
        }
    }

    pub fn redo(&mut self) -> Vec<Message> {
        let last = self.last.borrow();
        if let Some(next) = &last.next {
            let redo_messages = next.borrow().redo.clone();
            let new_last = Rc::clone(next);
            drop(last);
            self.last = new_last; // step forward
            redo_messages
        } else {
            // we reached the leaf node. Do nothing
            vec![]
        }
    }
}

/// A key to the end of the history.
///
/// A key is an object used by the update method to record “reverse” messages for a message
/// being processed. It can have one of three states, depending on whether messages should
/// or should not be put into it.
pub enum Key {
    /// This type of `Key` will correctly record any message put into it.
    Record(Node, Option<BatchMode>),

    /// This type of `Key` will panic when a message is put into it.
    /// Used when a message should not be recorded into the history, to protect against
    /// something trying to record it anyway.
    Fail,

    /// This type of `Key` will silently drop any messages put into it.
    /// Useful in case messages need to be replayed without anything being recorded.
    Dummy,
}

impl Key {
    /// Place a message into this key, to ultimately be played back when the history node
    /// it contains is undone.
    ///
    /// # Panics
    /// Panics if trying to record something into a `Fail` key.
    /// Also panics if a batch mode is specified that differs from an earlier one.
    pub fn put(&mut self, message: Message, batch_mode: BatchMode) {
        match self {
            Key::Record(node, old_batch_mode) => {
                node.undo.push(message);
                if let Some(old_batch_mode) = old_batch_mode {
                    assert_ne!(
                        *old_batch_mode, batch_mode,
                        "Tried to overwrite batch mode with a different one"
                    );
                    *old_batch_mode = batch_mode;
                } else {
                    *old_batch_mode = Some(batch_mode);
                }
            }
            Key::Fail => {
                panic!("Tried to record undo data for a message that should not be undone");
            }
            Key::Dummy => {
                // no-op
            }
        }
    }

    // Convenience functions for common batching scenarios
    // See `BatchMode` and `BatchAppendMode` for documentation on how these functions behave

    pub fn put_no_batch(&mut self, message: Message) {
        self.put(message, BatchMode::NoBatching);
    }

    pub fn put_instant(&mut self, message: Message) {
        self.put(
            message,
            BatchMode::Batched {
                undo: BatchAppendMode::Instant,
                redo: BatchAppendMode::Instant,
            },
        );
    }

    pub fn put_incremental(&mut self, message: Message) {
        self.put(
            message,
            BatchMode::Batched {
                undo: BatchAppendMode::Incremental,
                redo: BatchAppendMode::Incremental,
            },
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BatchMode {
    /// No batching will be performed at all.
    /// Every recorded message will be its own node in the history.
    NoBatching,

    /// Nodes will be batched together under certain conditions for better ergonomics
    /// (e.g. such that when a text is edited, every individual single-character edit doesn't
    /// become its own undo-redo node).
    /// The `BatchAppendMode` specifies whether messages are appended or overwritten.
    Batched {
        undo: BatchAppendMode,
        redo: BatchAppendMode,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum BatchAppendMode {
    /// Messages are logically treated as “setting” rather than “incrementally changing”
    /// the state, with the consequence that when batching, the later undo message is simply
    /// deleted, because the earlier undo message will restore both the previous edit and the
    /// current edit.
    /// For instance, this is suitable for a message where the destination data field is *set*
    /// to some value rather than incrementing/decrementing it.
    Instant,

    /// Nodes will be batched together, but all undo/redo messages will be retained and
    /// played back in the respective correct order. Suitable for messages that logically
    /// *increment*/*append to* a value.
    Incremental,
}
