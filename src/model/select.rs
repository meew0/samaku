use crate::subtitle::EventIndex;
use std::collections::HashSet;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EventSelection {
    /// Indices of currently selected events. May be any length, or empty if no event is currently
    /// selected.
    pub indices: HashSet<EventIndex>,

    /// Index of the event that was most recently selected, or `None` if none is selected.
    pub last: Option<EventIndex>,
}

impl EventSelection {
    /// Returns the “active” event: the most recently selected one,
    /// or if exactly one event is selected, that one.
    /// Otherwise, it returns `None`.
    ///
    /// # Panics
    /// Should not panic in normal operation.
    #[must_use]
    pub fn active(&self) -> Option<EventIndex> {
        self.last
            .or_else(|| (self.indices.len() == 1).then(|| *self.indices.iter().next().unwrap()))
    }

    #[must_use]
    pub fn contains(&self, event_index: EventIndex) -> bool {
        self.indices.contains(&event_index)
    }

    #[must_use]
    pub fn is_last(&self, event_index: EventIndex) -> bool {
        self.last == Some(event_index)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn set_single(
        &mut self,
        event_index: EventIndex,
        state: bool,
        last: Option<EventIndex>,
    ) -> (bool, Option<EventIndex>) {
        let old_state = self.contains(event_index);
        let old_last = self.last;

        if self.indices.contains(&event_index) && !state {
            self.indices.remove(&event_index);
        } else if !self.indices.contains(&event_index) && state {
            self.indices.insert(event_index);
        }
        self.last = last;

        (old_state, old_last)
    }

    pub fn select(&mut self, event_index: EventIndex) {
        self.indices.insert(event_index);
        self.last = Some(event_index);
    }

    pub fn select_from(&mut self, other: &Self) {
        self.indices.extend(other.indices.iter().copied());
        self.last = other.last.or(self.last);
    }

    /// Deselects the given event. Returns what was actually deselected.
    pub fn deselect(&mut self, event_index: EventIndex) -> Option<EventIndex> {
        let was_present = self.indices.remove(&event_index);
        if self.last == Some(event_index) {
            self.last = None;
        }
        was_present.then_some(event_index)
    }

    /// Deselects all events in the iterator. Returns a set of events that were actually deselected.
    #[expect(
        clippy::return_self_not_must_use,
        reason = "does not always need to be used in this case"
    )]
    pub fn deselect_all<I: Iterator<Item = EventIndex>>(&mut self, event_indices: I) -> Self {
        let old_last = self.last;

        let mut deselected = if let (_, Some(len)) = event_indices.size_hint() {
            HashSet::with_capacity(len)
        } else {
            HashSet::new()
        };
        for event_index in event_indices {
            if let Some(deselected_index) = self.deselect(event_index) {
                deselected.insert(deselected_index);
            }
        }

        // Only return a new `last` event if the current one was actually deselected
        let new_last = if let Some(old_last) = old_last
            && deselected.contains(&old_last)
        {
            Some(old_last)
        } else {
            None
        };

        Self {
            indices: deselected,
            last: new_last,
        }
    }

    /// Clears the selection. Returns the current selection for further use.
    #[expect(
        clippy::return_self_not_must_use,
        reason = "does not always need to be used in this case"
    )]
    pub fn clear(&mut self) -> Self {
        let old_indices = std::mem::take(&mut self.indices);
        let old_last = std::mem::take(&mut self.last);
        Self {
            indices: old_indices,
            last: old_last,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = EventIndex> {
        <&Self as IntoIterator>::into_iter(self)
    }
}

impl<'a> IntoIterator for &'a EventSelection {
    type Item = EventIndex;

    type IntoIter = core::iter::Copied<std::collections::hash_set::Iter<'a, Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        self.indices.iter().copied()
    }
}
