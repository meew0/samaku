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
    name: &'static str,
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
            name: "",
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
            last: Self::root_node(),
        }
    }

    fn root_node() -> Rc<RefCell<Node>> {
        Rc::new(RefCell::new(Node::root()))
    }

    fn last(&self) -> Rc<RefCell<Node>> {
        Rc::clone(&self.last)
    }

    fn make_leaf(&self, message: Message) -> Node {
        let discriminant = std::mem::discriminant(&message);
        Node {
            name: "",
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
    pub fn make_key(&self, message: &Message) -> Key {
        match *message {
            // messages that might eventually be recorded in the history
            Message::CreateStyle
            | Message::DeleteStyle(_)
            | Message::RestoreStyle(_, _, _)
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
            | Message::DeleteEvents(_)
            | Message::DeleteSelectedEvents
            | Message::RestoreEvents(_, _)
            | Message::ToggleEventSelection(_)
            | Message::GroupSelectEvents(_, _, _)
            | Message::SetEventSelectionSingle(_, _, _)
            | Message::SelectOnlyEvent(_)
            | Message::SetEventSelection(_)
            | Message::SelectAllEvents
            | Message::MultiEditEventText(_)
            | Message::MultiEditEventActor(_)
            | Message::MultiEditEventEffect(_)
            | Message::MultiEditEventStyleIndex(_)
            | Message::MultiEditEventLayerIndex(_)
            | Message::MultiEditEventType(_)
            | Message::MultiEditEventStartTime(_)
            | Message::MultiEditEventDuration(_)
            | Message::SetEventStartTimeAndDuration(_, _, _)
            | Message::TextEditorActionPerformed(_, _)
            | Message::CreateEmptyFilterAndAssignToSelected
            | Message::AssignFilterToEvents(_, _)
            | Message::UnassignFilterFromEvents(_, _)
            | Message::AssignFilterToSelectedEvents(_)
            | Message::UnassignFilterFromSelectedEvents(_)
            | Message::SetFilterName(_, _)
            | Message::SetFilterGraph(_, _)
            | Message::DeleteFilter(_)
            | Message::RestoreFilter(_, _, _)
            | Message::AddNode(_, _)
            | Message::DeleteNodes(_, _)
            | Message::MoveNode(_, _, _)
            | Message::MoveNodeGroup(_, _, _)
            | Message::ConnectNodes(_, _, _)
            | Message::DisconnectNodes(_, _, _)
            | Message::SetNodeConnection(_, _, _)
            | Message::UpdateReticulePosition(_, _)
            | Message::CreateTrack
            | Message::DeleteTrack(_)
            | Message::SetTrackName(_, _)
            | Message::TrackMotionForSelectedTracks(_, _, _)
            | Message::ToggleTrackSelection(_)
            | Message::SetTrackSelectionSingle(_, _, _)
            | Message::SelectOnlyTrack(_)
            | Message::SetTrackSelection(_)
            | Message::SelectAllTracks => {
                let cloned = message.clone();
                let node = self.make_leaf(cloned);
                Key::Record(node, None)
            }
            // messages that will never need to be recorded in the history
            Message::None
            | Message::Batch(_)
            | Message::Pane(_, _)
            | Message::FocusedPane(_)
            | Message::Node(_, _, _)
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
            | Message::DeselectEvents(_, _)
            | Message::DeselectTracks(_, _)
            | Message::MultiAssignFiltersToEvents(_)
            | Message::ActivateNodes(_, _)
            | Message::TrackMotionForNode(_, _, _)
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
        if let Key::Record(mut node, batch_mode_option) = key {
            if node.undo.is_empty() {
                // No data was put into this node
                // (almost certainly because undo/redo is NYI for this particular message)
                println!(
                    "Missing undo information for node: {:?}",
                    node.redo.last().unwrap().name()
                );
                return;
            }

            let batch_mode = batch_mode_option
                .expect("A batch mode should be specified if messages have been appended");

            let prev = node.prev.as_ref().expect("tried to record unlinked node");
            assert!(
                Rc::ptr_eq(prev, &self.last),
                "tried to record node not created from history leaf"
            );

            // We need to reverse the new undo messages, since batched undo messages
            // are logically played backwards, but we want to preserve the order
            // the messages were put into the node.
            node.undo.reverse();

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
                        assert!(
                            !self.last.borrow().undo.is_empty(),
                            "currently present undo list should not be empty"
                        );
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
        if let &Some(ref prev) = &last.prev {
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

    #[must_use]
    pub fn peek_undo(&self) -> Option<&'static str> {
        let last = self.last.borrow();
        // If a previous node exists (i.e. we have not reached the root),
        // return the name of the current (last) node,
        // since that is the one that will be undone.
        last.prev.as_ref().map(|_| last.name)
    }

    pub fn redo(&mut self) -> Vec<Message> {
        let last = self.last.borrow();
        if let &Some(ref next) = &last.next {
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

    #[must_use]
    pub fn peek_redo(&self) -> Option<&'static str> {
        let last = self.last.borrow();
        last.next.as_ref().map(|next| next.borrow().name)
    }

    /// Removes all history nodes.
    pub fn clear(&mut self) {
        self.last = Self::root_node();
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
    pub fn put(&mut self, name: &'static str, message: Message, batch_mode: BatchMode) {
        match *self {
            Key::Record(ref mut node, ref mut old_batch_mode_option) => {
                node.name = name;
                node.undo.push(message);
                if let &mut Some(ref mut old_batch_mode) = old_batch_mode_option {
                    assert_eq!(
                        *old_batch_mode, batch_mode,
                        "Tried to overwrite batch mode with a different one"
                    );
                    *old_batch_mode = batch_mode;
                } else {
                    *old_batch_mode_option = Some(batch_mode);
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

    pub fn put_no_batch(&mut self, name: &'static str, message: Message) {
        self.put(name, message, BatchMode::NoBatching);
    }

    pub fn put_instant(&mut self, name: &'static str, message: Message) {
        self.put(
            name,
            message,
            BatchMode::Batched {
                undo: BatchAppendMode::Instant,
                redo: BatchAppendMode::Instant,
            },
        );
    }

    pub fn put_incremental(&mut self, name: &'static str, message: Message) {
        self.put(
            name,
            message,
            BatchMode::Batched {
                undo: BatchAppendMode::Incremental,
                redo: BatchAppendMode::Incremental,
            },
        );
    }

    /// Instead of using the message the node was created with,
    /// use the specified message when redoing the node.
    ///
    /// # Panics
    /// Panics if trying to record something into a `Fail` key.
    pub fn override_redo(&mut self, redo_message: Message) {
        match *self {
            Key::Record(ref mut node, _) => {
                node.redo.clear();
                node.redo.push(redo_message);
            }
            Key::Fail => {
                panic!("Tried to record undo data for a message that should not be undone");
            }
            Key::Dummy => {
                // no-op
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches2::assert_matches;

    // Helpers

    /// Record one action into history with NoBatching.
    fn record_no_batch(history: &mut History, redo_msg: &Message, undo_msg: Message) {
        let mut key = history.make_key(redo_msg);
        key.put("test", undo_msg, BatchMode::NoBatching);
        history.record(key);
    }

    /// Record one action into history with a given batching mode.
    fn record_batched(
        history: &mut History,
        redo_msg: &Message,
        undo_msg: Message,
        undo_mode: BatchAppendMode,
        redo_mode: BatchAppendMode,
    ) {
        let mut key = history.make_key(redo_msg);
        key.put(
            "test",
            undo_msg,
            BatchMode::Batched {
                undo: undo_mode,
                redo: redo_mode,
            },
        );
        history.record(key);
    }

    // Tests

    #[test]
    fn new_history() {
        let mut hist = History::new();
        assert!(hist.undo().is_empty(), "undo at root should be empty");

        let mut hist = History::new();
        assert!(hist.redo().is_empty(), "redo at leaf should be empty");

        // Both should start at the root with no history.
        let mut hist = History::default();
        assert!(hist.undo().is_empty());
        assert!(hist.redo().is_empty());
    }

    #[test]
    fn make_key() {
        let hist = History::new();
        assert!(matches!(
            hist.make_key(&Message::AddEvent),
            Key::Record(_, _)
        ));

        let hist = History::new();
        assert!(matches!(hist.make_key(&Message::None), Key::Fail));

        // The Record variant from make_key begins with an empty undo vec and a
        // one-element redo vec containing the original message.
        let hist = History::new();
        if let Key::Record(node, batch_mode) = hist.make_key(&Message::AddEvent) {
            assert!(node.undo.is_empty(), "undo should be empty before put()");
            assert_eq!(
                node.redo.len(),
                1,
                "redo should contain the original message"
            );
            assert!(matches!(node.redo[0], Message::AddEvent));
            assert!(
                batch_mode.is_none(),
                "batch mode should be None before put()"
            );
        } else {
            panic!("expected Key::Record");
        }
    }

    #[test]
    fn record() {
        let mut hist = History::new();
        let key = hist.make_key(&Message::AddEvent);
        hist.record(key); // no put() → undo is empty → record does nothing
        assert!(hist.undo().is_empty(), "history should still be at root");
    }

    #[test]
    fn key_dummy() {
        let mut key = Key::Dummy;
        // put() on a Dummy key must not panic and must have no observable effect
        key.put("test", Message::AddEvent, BatchMode::NoBatching);

        let mut key = Key::Dummy;
        key.put("test", Message::AddEvent, BatchMode::NoBatching);
        key.put("test", Message::DeleteSelectedEvents, BatchMode::NoBatching);
    }

    #[test]
    #[should_panic(expected = "should not be undone")]
    fn key_fail_panics_on_put() {
        let mut key = Key::Fail;
        key.put("test", Message::AddEvent, BatchMode::NoBatching);
    }

    #[test]
    fn put() {
        // Test undo message being added
        let hist = History::new();
        if let Key::Record(ref mut node, ref mut batch_mode) = hist.make_key(&Message::AddEvent) {
            node.undo.push(Message::DeleteSelectedEvents);
            *batch_mode = Some(BatchMode::NoBatching);
            assert_eq!(node.undo.len(), 1);
            assert!(matches!(node.undo[0], Message::DeleteSelectedEvents));
        } else {
            panic!("expected Key::Record");
        }

        let hist = History::new();
        let mut key = hist.make_key(&Message::AddEvent);
        key.put_no_batch("test", Message::DeleteSelectedEvents);
        if let Key::Record(node, batch_mode) = key {
            assert_eq!(node.undo.len(), 1);
            assert_eq!(batch_mode, Some(BatchMode::NoBatching));
        } else {
            panic!("expected Key::Record");
        }
    }

    #[test]
    fn undo_redo_basic() {
        // single action undo
        let mut hist = History::new();
        record_no_batch(&mut hist, &Message::AddEvent, Message::DeleteSelectedEvents);

        let undo = hist.undo();
        assert_eq!(undo.len(), 1);
        assert!(matches!(undo[0], Message::DeleteSelectedEvents));

        // move back to root
        let mut hist = History::new();
        record_no_batch(&mut hist, &Message::AddEvent, Message::DeleteSelectedEvents);
        drop(hist.undo());
        // Now at root — undo again should be empty
        assert!(hist.undo().is_empty());

        // single action redo
        let mut hist = History::new();
        record_no_batch(&mut hist, &Message::AddEvent, Message::DeleteSelectedEvents);
        drop(hist.undo());

        let redo = hist.redo();
        assert_eq!(redo.len(), 1);
        assert!(matches!(redo[0], Message::AddEvent));

        // move forward to leaf
        let mut hist = History::new();
        record_no_batch(&mut hist, &Message::AddEvent, Message::DeleteSelectedEvents);
        drop(hist.undo());
        drop(hist.redo());
        // Now at the leaf — redo again should be empty
        assert!(hist.redo().is_empty());
    }

    #[test]
    fn undo_redo_2() {
        let mut hist = History::new();
        record_no_batch(&mut hist, &Message::AddEvent, Message::DeleteSelectedEvents);
        record_no_batch(&mut hist, &Message::CreateStyle, Message::DeleteStyle(0));

        // Undo second action
        let undo2 = hist.undo();
        assert_eq!(undo2.len(), 1);
        assert!(matches!(undo2[0], Message::DeleteStyle(0)));

        // Undo first action
        let undo1 = hist.undo();
        assert_eq!(undo1.len(), 1);
        assert!(matches!(undo1[0], Message::DeleteSelectedEvents));

        // At root
        assert!(hist.undo().is_empty());

        // Redo first action
        let redo1 = hist.redo();
        assert_eq!(redo1.len(), 1);
        assert!(matches!(redo1[0], Message::AddEvent));

        // Redo second action
        let redo2 = hist.redo();
        assert_eq!(redo2.len(), 1);
        assert!(matches!(redo2[0], Message::CreateStyle));

        // At leaf
        assert!(hist.redo().is_empty());
    }

    #[test]
    fn undo_redo_3() {
        let mut hist = History::new();
        record_no_batch(&mut hist, &Message::AddEvent, Message::DeleteSelectedEvents);
        record_no_batch(&mut hist, &Message::CreateStyle, Message::DeleteStyle(0));
        record_no_batch(
            &mut hist,
            &Message::SetStyleName(0, "hello".into()),
            Message::SetStyleName(0, String::new()),
        );

        // Undo all three
        assert_eq!(hist.undo().len(), 1); // undo 3rd
        assert_eq!(hist.undo().len(), 1); // undo 2nd
        assert_eq!(hist.undo().len(), 1); // undo 1st
        assert!(hist.undo().is_empty()); // at root

        // Redo all three
        assert_eq!(hist.redo().len(), 1);
        assert_eq!(hist.redo().len(), 1);
        assert_eq!(hist.redo().len(), 1);
        assert!(hist.redo().is_empty()); // at leaf
    }

    #[test]
    fn undo_redo_peek() {
        let mut hist = History::new();

        let mut key = hist.make_key(&Message::AddEvent);
        key.put(
            "earlier",
            Message::DeleteSelectedEvents,
            BatchMode::NoBatching,
        );
        hist.record(key);

        let mut key = hist.make_key(&Message::CreateStyle);
        key.put("later", Message::DeleteStyle(0), BatchMode::NoBatching);
        hist.record(key);

        assert_matches!(hist.peek_undo(), Some("later"));
        assert_matches!(hist.peek_redo(), None);

        // Undo second action
        let undo2 = hist.undo();
        assert_eq!(undo2.len(), 1);
        assert!(matches!(undo2[0], Message::DeleteStyle(0)));

        assert_matches!(hist.peek_undo(), Some("earlier"));
        assert_matches!(hist.peek_redo(), Some("later"));

        // Undo first action
        let undo1 = hist.undo();
        assert_eq!(undo1.len(), 1);
        assert!(matches!(undo1[0], Message::DeleteSelectedEvents));

        // At root
        assert_matches!(hist.peek_undo(), None);
        assert_matches!(hist.peek_redo(), Some("earlier"));
    }

    #[test]
    fn redo_discard() {
        let mut hist = History::new();
        record_no_batch(&mut hist, &Message::AddEvent, Message::DeleteSelectedEvents);
        record_no_batch(&mut hist, &Message::CreateStyle, Message::DeleteStyle(0));

        // Undo back to after action 1
        drop(hist.undo());

        // Record a new, different action (action C)
        record_no_batch(
            &mut hist,
            &Message::SetStyleName(0, "new".into()),
            Message::SetStyleName(0, String::new()),
        );

        // Now at node C (the leaf). Undo back to node A so that redo can be tested.
        drop(hist.undo());

        // Redo should lead to action C, not the original (discarded) action B
        let redo = hist.redo();
        assert_eq!(redo.len(), 1);
        assert!(
            matches!(&redo[0], Message::SetStyleName(0, text) if text == "new"),
            "redo should return the newly recorded action, not the discarded branch"
        );

        let mut hist = History::new();
        record_no_batch(&mut hist, &Message::AddEvent, Message::DeleteSelectedEvents);
        record_no_batch(&mut hist, &Message::CreateStyle, Message::DeleteStyle(0));

        drop(hist.undo()); // undo action 2, now at node 1
        record_no_batch(
            &mut hist,
            &Message::SetStyleName(0, "x".into()),
            Message::SetStyleName(0, String::new()),
        );

        // After redoing the new action, there should be nothing more to redo
        drop(hist.redo());
        assert!(
            hist.redo().is_empty(),
            "no further redo entries after new branch"
        );
    }

    #[test]
    fn batch_modes() {
        // No batching
        let mut hist = History::new();
        record_no_batch(&mut hist, &Message::AddEvent, Message::DeleteSelectedEvents);
        record_no_batch(&mut hist, &Message::AddEvent, Message::DeleteSelectedEvents);

        // Two separate undo steps
        let undo2 = hist.undo();
        assert_eq!(undo2.len(), 1);
        let undo1 = hist.undo();
        assert_eq!(undo1.len(), 1);
        assert!(hist.undo().is_empty(), "should be at root after two undos");

        // Instant
        let mut hist = History::new();
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "a".into()),
            Message::SetStyleName(0, "original".into()),
            BatchAppendMode::Instant,
            BatchAppendMode::Instant,
        );
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "b".into()),
            Message::SetStyleName(0, "original".into()),
            BatchAppendMode::Instant,
            BatchAppendMode::Instant,
        );

        // Both actions should be merged: only ONE undo step
        let undo = hist.undo();
        assert_eq!(
            undo.len(),
            1,
            "instant batching should produce one undo message"
        );
        assert!(
            hist.undo().is_empty(),
            "should be at root after a single undo"
        );

        // In Instant mode the first undo message is kept unchanged, since it
        // already restores the state from before the first edit.
        let mut hist = History::new();
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "a".into()),
            Message::SetStyleName(0, "before_a".into()),
            BatchAppendMode::Instant,
            BatchAppendMode::Instant,
        );
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "b".into()),
            Message::SetStyleName(0, "before_b_should_be_ignored".into()),
            BatchAppendMode::Instant,
            BatchAppendMode::Instant,
        );

        let undo = hist.undo();
        assert_eq!(undo.len(), 1);
        assert!(
            matches!(&undo[0], Message::SetStyleName(0, text) if text == "before_a"),
            "the first undo message should be retained in Instant mode"
        );

        // In Instant mode the redo vector is replaced with the latest message.
        let mut hist = History::new();
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "a".into()),
            Message::SetStyleName(0, "original".into()),
            BatchAppendMode::Instant,
            BatchAppendMode::Instant,
        );
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "b".into()),
            Message::SetStyleName(0, "original".into()),
            BatchAppendMode::Instant,
            BatchAppendMode::Instant,
        );

        // Undo, then redo: should return the SECOND (latest) redo message
        drop(hist.undo());
        let redo = hist.redo();
        assert_eq!(redo.len(), 1);
        assert!(
            matches!(&redo[0], Message::SetStyleName(0, text) if text == "b"),
            "redo should return the latest message in Instant mode"
        );

        // Incremental
        let mut hist = History::new();
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "a".into()),
            Message::SetStyleName(0, "undo_a".into()),
            BatchAppendMode::Incremental,
            BatchAppendMode::Incremental,
        );
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "b".into()),
            Message::SetStyleName(0, "undo_b".into()),
            BatchAppendMode::Incremental,
            BatchAppendMode::Incremental,
        );

        // Should be ONE undo step (batched)
        let undo = hist.undo();
        assert_eq!(
            undo.len(),
            2,
            "incremental batching accumulates undo messages"
        );
        assert!(
            hist.undo().is_empty(),
            "should be at root after one batched undo"
        );

        let mut hist = History::new();
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "a".into()),
            Message::SetStyleName(0, "undo_a".into()),
            BatchAppendMode::Incremental,
            BatchAppendMode::Incremental,
        );
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "b".into()),
            Message::SetStyleName(0, "undo_b".into()),
            BatchAppendMode::Incremental,
            BatchAppendMode::Incremental,
        );

        let undo = hist.undo();
        assert_eq!(undo.len(), 2);
        assert!(matches!(&undo[0], Message::SetStyleName(0, text) if text == "undo_a"));
        assert!(matches!(&undo[1], Message::SetStyleName(0, text) if text == "undo_b"));

        let mut hist = History::new();
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "a".into()),
            Message::SetStyleName(0, "undo_a".into()),
            BatchAppendMode::Incremental,
            BatchAppendMode::Incremental,
        );
        record_batched(
            &mut hist,
            &Message::SetStyleName(0, "b".into()),
            Message::SetStyleName(0, "undo_b".into()),
            BatchAppendMode::Incremental,
            BatchAppendMode::Incremental,
        );

        drop(hist.undo());
        let redo = hist.redo();
        assert_eq!(
            redo.len(),
            2,
            "incremental batching accumulates redo messages"
        );
        assert!(matches!(&redo[0], Message::SetStyleName(0, text) if text == "a"));
        assert!(matches!(&redo[1], Message::SetStyleName(0, text) if text == "b"));

        let mut hist = History::new();
        record_batched(
            &mut hist,
            &Message::AddEvent,
            Message::DeleteSelectedEvents,
            BatchAppendMode::Instant,
            BatchAppendMode::Instant,
        );
        record_batched(
            &mut hist,
            &Message::CreateStyle, // different type → no merge
            Message::DeleteStyle(0),
            BatchAppendMode::Instant,
            BatchAppendMode::Instant,
        );

        let undo2 = hist.undo();
        assert_eq!(undo2.len(), 1);
        assert!(matches!(undo2[0], Message::DeleteStyle(0)));

        let undo1 = hist.undo();
        assert_eq!(undo1.len(), 1);
        assert!(matches!(undo1[0], Message::DeleteSelectedEvents));

        assert!(hist.undo().is_empty());

        let mut hist = History::new();
        record_batched(
            &mut hist,
            &Message::AddEvent,
            Message::DeleteSelectedEvents,
            BatchAppendMode::Incremental,
            BatchAppendMode::Incremental,
        );
        record_batched(
            &mut hist,
            &Message::CreateStyle,
            Message::DeleteStyle(0),
            BatchAppendMode::Incremental,
            BatchAppendMode::Incremental,
        );

        assert_eq!(hist.undo().len(), 1); // second node, different type
        assert_eq!(hist.undo().len(), 1); // first node
        assert!(hist.undo().is_empty());

        // Three messages
        let mut hist = History::new();
        for idx in 0_u8..3 {
            record_batched(
                &mut hist,
                &Message::SetStyleName(0, format!("state_{idx}")),
                Message::SetStyleName(0, format!("undo_{idx}")),
                BatchAppendMode::Incremental,
                BatchAppendMode::Incremental,
            );
        }

        let undo = hist.undo();
        assert_eq!(
            undo.len(),
            3,
            "all three undo messages should be accumulated"
        );
        assert!(hist.undo().is_empty());

        let redo = hist.redo();
        assert_eq!(
            redo.len(),
            3,
            "all three redo messages should be accumulated"
        );
    }

    #[test]
    fn undo_redo_symmetry() {
        const STEP_COUNT: usize = 5;
        let mut hist = History::new();
        for idx in 0..STEP_COUNT {
            record_no_batch(
                &mut hist,
                &Message::SetStyleName(0, format!("state_{idx}")),
                Message::SetStyleName(0, format!("undo_{idx}")),
            );
        }

        // Undo all
        for _ in 0..STEP_COUNT {
            assert!(!hist.undo().is_empty());
        }
        assert!(hist.undo().is_empty(), "should be at root");

        // Redo all
        for _ in 0..STEP_COUNT {
            assert!(!hist.redo().is_empty());
        }
        assert!(hist.redo().is_empty(), "should be at leaf");
    }

    #[test]
    #[should_panic(expected = "Tried to overwrite batch mode with a different one")]
    fn put_different_modes() {
        let hist = History::new();
        let mut key = hist.make_key(&Message::AddEvent);
        key.put("test", Message::DeleteSelectedEvents, BatchMode::NoBatching);
        // Second put with different mode triggers assert_ne! in Key::put
        key.put(
            "test",
            Message::DeleteSelectedEvents,
            BatchMode::Batched {
                undo: BatchAppendMode::Incremental,
                redo: BatchAppendMode::Incremental,
            },
        );
    }
}
