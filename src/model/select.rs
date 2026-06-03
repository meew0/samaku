use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Selection<T: Debug + Copy + Eq + Hash> {
    /// Indices of currently selected events. May be any length, or empty if no event is currently
    /// selected.
    pub indices: HashSet<T>,

    /// Index of the event that was most recently selected, or `None` if none is selected.
    pub last: Option<T>,
}

impl<T: Debug + Copy + Eq + Hash> Selection<T> {
    #[must_use]
    pub fn from_indices(indices: HashSet<T>) -> Self {
        Self {
            indices,
            last: None,
        }
    }

    /// Returns the “active” event: the most recently selected one,
    /// or if exactly one event is selected, that one.
    /// Otherwise, it returns `None`.
    ///
    /// # Panics
    /// Should not panic in normal operation.
    #[must_use]
    pub fn active(&self) -> Option<T> {
        self.last
            .or_else(|| (self.indices.len() == 1).then(|| *self.indices.iter().next().unwrap()))
    }

    #[must_use]
    pub fn contains(&self, index: T) -> bool {
        self.indices.contains(&index)
    }

    #[must_use]
    pub fn is_last(&self, index: T) -> bool {
        self.last == Some(index)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn set_single(&mut self, index: T, state: bool, last: Option<T>) -> (bool, Option<T>) {
        let old_state = self.contains(index);
        let old_last = self.last;

        if self.indices.contains(&index) && !state {
            self.indices.remove(&index);
        } else if !self.indices.contains(&index) && state {
            self.indices.insert(index);
        } else {
            // The current selection state is already equal to the desired state.
            // Nothing to do.
        }
        self.last = last;

        (old_state, old_last)
    }

    /// Selects the given items. Returns what was actually selected
    /// (`None` if the given item was already selected).
    pub fn select(&mut self, event_index: T) -> Option<T> {
        let was_inserted = self.indices.insert(event_index);
        self.last = Some(event_index);
        was_inserted.then_some(event_index)
    }

    /// Selects the given items.
    /// Returns a set of items that were actually selected, and the previous last item.
    pub fn select_all<I: Iterator<Item = T>>(&mut self, indices: I) -> (HashSet<T>, Option<T>) {
        let mut selected = if let (_, Some(len)) = indices.size_hint() {
            self.indices.reserve(len);
            HashSet::with_capacity(len)
        } else {
            HashSet::new()
        };

        let old_last = self.last;

        for index in indices {
            if let Some(selected_index) = self.select(index) {
                selected.insert(selected_index);
            }
        }

        (selected, old_last)
    }

    pub fn select_from(&mut self, other: &Self) {
        self.indices.extend(other.indices.iter().copied());
        self.last = other.last.or(self.last);
    }

    /// Deselects the given item. Returns what was actually deselected.
    pub fn deselect(&mut self, event_index: T) -> Option<T> {
        let was_present = self.indices.remove(&event_index);
        if self.last == Some(event_index) {
            self.last = None;
        }
        was_present.then_some(event_index)
    }

    /// Deselects all items in the iterator. Returns a set of items that were actually deselected.
    #[expect(
        clippy::return_self_not_must_use,
        reason = "does not always need to be used in this case"
    )]
    pub fn deselect_all<I: Iterator<Item = T>>(&mut self, indices: I) -> Self {
        let old_last = self.last;

        let mut deselected = if let (_, Some(len)) = indices.size_hint() {
            HashSet::with_capacity(len)
        } else {
            HashSet::new()
        };
        for index in indices {
            if let Some(deselected_index) = self.deselect(index) {
                deselected.insert(deselected_index);
            }
        }

        // Only return a new `last` item if the current one was actually deselected
        let new_last = if let Some(old_last_index) = old_last
            && deselected.contains(&old_last_index)
        {
            Some(old_last_index)
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

    pub fn iter(&self) -> impl Iterator<Item = T> {
        <&Self as IntoIterator>::into_iter(self)
    }
}

impl<T: Debug + Copy + Eq + Hash> Default for Selection<T> {
    fn default() -> Self {
        Self {
            indices: HashSet::new(),
            last: None,
        }
    }
}

impl<'a, T: Debug + Copy + Eq + Hash> IntoIterator for &'a Selection<T> {
    type Item = T;

    type IntoIter = core::iter::Copied<std::collections::hash_set::Iter<'a, Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        self.indices.iter().copied()
    }
}
