use crate::subtitle::{Duration, Event, Extradata, StartTime, compile};
use crate::{message, nde};
use std::collections::HashSet;
use std::fmt::Debug;
use std::ops::{Index, IndexMut, Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EventIndex(usize);

/// Ordered collection of [`Event`]s.
///
/// Internally, an `EventTrack` is a combination of 3 data structures:
///  - an array (`Vec`) to hold the event data itself in an unordered fashion;
///  - an ordered hash table (`indexmap::IndexSet`) to store the order of the array items;
///  - and an augmented AVL interval tree (`interavl::IntervalTree`) for logarithmic interval-based indexing.
///
/// To ensure the array can serve as a stable lookup table for indices stored as values in the other 2 data structures,
/// the order of entries in the array is never changed. Entries are only ever appended at the end of the array, and if
/// an event is deleted, its corresponding entry is simply nulled without changing the rest of the array.
///
/// Note that a key invariant that must be upheld on the caller side is that event timing data (`start`/`duration`)
/// should never be manually changed within event data; instead, use `update_event_times`.
pub struct EventTrack {
    events: Vec<Option<Event<'static>>>,
    query_index: interavl::IntervalTree<StartTime, Leaf>,
    order: indexmap::IndexSet<EventIndex>,
    count: usize,
}

impl EventTrack {
    /// Create a new empty `EventTrack`.
    #[must_use]
    pub fn new_empty() -> Self {
        Self {
            events: vec![],
            query_index: interavl::IntervalTree::default(),
            order: indexmap::IndexSet::default(),
            count: 0,
        }
    }

    /// Returns true if and only if there are no events in this track.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Returns the number of events in the track.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Index into the track given the (opaque) `EventIndex`.
    #[must_use]
    pub fn get(&mut self, index: EventIndex) -> Option<&Event<'static>> {
        self.events.get(index.0).and_then(Option::as_ref)
    }

    #[must_use]
    pub fn get_mut(&mut self, index: EventIndex) -> Option<&mut Event<'static>> {
        self.events.get_mut(index.0).and_then(Option::as_mut)
    }

    /// Get the `n`th entry of the track in order.
    ///
    /// # Panics
    /// Panics if `n` is out of bounds.
    #[must_use]
    pub fn nth(&self, n: usize) -> (EventIndex, &Event<'static>) {
        self.get_nth(n).unwrap()
    }

    /// Get the `n`th entry of the track in order, checked.
    ///
    /// # Panics
    /// Should not panic in normal use, only if the invariants of `EventTrack` are somehow violated.
    #[must_use]
    pub fn get_nth(&self, n: usize) -> Option<(EventIndex, &Event<'static>)> {
        let index = self.order.get_index(n).copied();
        index.map(|index| (index, self.events[index.0].as_ref().unwrap()))
    }

    /// Update the times (start time and duration) of the given event, upholding invariants required for correct behavior of `EventTrack`.
    ///
    /// # Panics
    /// Panics if the given event index is invalid.
    pub fn update_event_times(
        &mut self,
        event_index: EventIndex,
        start: StartTime,
        duration: Duration,
    ) {
        // Find the event so we know what interval to remove.
        let event = self.events[event_index.0].as_mut().unwrap();
        let interval = event.time_range();

        // Remove the interval.
        Self::internal_query_index_remove(&mut self.query_index, interval, event_index);

        // Update the event itself
        event.start = start;
        event.duration = duration;

        // Re-add the interval
        let new_interval = event.time_range();
        Self::internal_query_index_insert_merge(&mut self.query_index, new_interval, event_index);
        debug_assert!(self.check_invariants());
    }

    fn check_invariants(&self) -> bool {
        debug_assert_eq!(self.count, self.order.len());
        debug_assert_eq!(
            self.count,
            self.events.iter().filter(|x| x.is_some()).count()
        );
        debug_assert_eq!(
            self.count,
            self.query_index.iter().map(|(_, leaf)| leaf.count()).sum()
        );

        for (i, event) in self
            .events
            .iter()
            .enumerate()
            .filter(|(_, maybe_event)| maybe_event.is_some())
        {
            let event_index = EventIndex(i);
            debug_assert!(self.order.contains(&event_index));
            debug_assert!(
                self.query_index
                    .get(&event.as_ref().unwrap().time_range())
                    .unwrap()
                    .contains(event_index)
            );
        }

        true
    }

    fn internal_query_index_remove(
        query_index: &mut interavl::IntervalTree<StartTime, Leaf>,
        interval: Range<StartTime>,
        event_index: EventIndex,
    ) {
        if let Some(Leaf::Multiple(mut vec)) = query_index.remove(&interval) {
            // If it turned out that we removed multiple items (Leaf::Multiple), remove the index we want from the leaf...
            let index_in_vec = vec.iter().position(|x| *x == event_index).unwrap();
            vec.swap_remove(index_in_vec);

            // ...and add a new leaf with fewer items
            query_index.insert(
                interval,
                if vec.len() == 1 {
                    Leaf::Single(vec[0])
                } else {
                    Leaf::Multiple(vec)
                },
            );
        }
    }

    fn internal_query_index_insert_merge(
        query_index: &mut interavl::IntervalTree<StartTime, Leaf>,
        interval: Range<StartTime>,
        event_index: EventIndex,
    ) {
        if let Some(old) = query_index.insert(interval.clone(), Leaf::Single(event_index)) {
            // If it turned out there already was something there, add the correct number of items back
            query_index.insert(
                interval,
                match old {
                    Leaf::Single(old_event_index) => {
                        Leaf::Multiple(vec![old_event_index, event_index])
                    }
                    Leaf::Multiple(mut vec) => {
                        vec.push(event_index);
                        Leaf::Multiple(vec)
                    }
                },
            );
        }
    }

    /// Add a new event to the end of the track.
    ///
    /// # Panics
    /// Should not panic in normal use, only if the invariants of `EventTrack` are somehow violated.
    pub fn push(&mut self, event: Event<'static>) -> EventIndex {
        let new_index = self.internal_insert(event);
        assert!(self.order.insert(new_index));
        debug_assert!(self.check_invariants());
        new_index
    }

    /// Add a new event to the track at the given index.
    ///
    /// # Panics
    /// Panics if the given index could not be found within the track.
    pub fn insert(&mut self, index: EventIndex, event: Event<'static>) -> EventIndex {
        let new_index = self.internal_insert(event);

        // Find the current position of the given event and move the new event there
        let pos = self.order.get_index_of(&index).unwrap();
        assert!(self.order.shift_insert(pos, new_index));

        debug_assert!(self.check_invariants());
        new_index
    }

    // Insert only into events and query index, not order
    fn internal_insert(&mut self, event: Event<'static>) -> EventIndex {
        let new_index = EventIndex(self.events.len());
        Self::internal_query_index_insert_merge(
            &mut self.query_index,
            event.time_range(),
            new_index,
        );
        self.events.push(Some(event));
        self.count += 1;
        new_index
    }

    /// Remove all events whose indices are contained in the given set. Clears the set afterwards
    /// (since the indices it references are no longer valid); hence, it requires a mutable
    /// reference to the set.
    ///
    /// # Panics
    /// Panics if any of the indices is invalid.
    pub fn remove_from_set(&mut self, set: &mut HashSet<EventIndex>) {
        self.order.retain(|event_index| !set.contains(event_index));
        for event_index in set.iter() {
            let event = self.events[event_index.0].take().unwrap();
            let interval = event.time_range();
            Self::internal_query_index_remove(&mut self.query_index, interval, *event_index);
        }

        self.count -= set.len();
        set.clear();
        debug_assert!(self.check_invariants());
    }

    /// If exactly one event is selected, this method returns the index of that element. Otherwise,
    /// it returns `None`.
    ///
    /// # Panics
    /// Should not panic in normal operation.
    #[must_use]
    pub fn active_event_index(selected_event_indices: &HashSet<EventIndex>) -> Option<EventIndex> {
        (selected_event_indices.len() == 1).then(|| *selected_event_indices.iter().next().unwrap())
    }

    #[must_use]
    pub fn active_event(
        &self,
        selected_event_indices: &HashSet<EventIndex>,
    ) -> Option<&Event<'static>> {
        Self::active_event_index(selected_event_indices).map(|index| &self[index])
    }

    #[must_use]
    pub fn active_event_mut(
        &mut self,
        selected_event_indices: &HashSet<EventIndex>,
    ) -> Option<&mut Event<'static>> {
        Self::active_event_index(selected_event_indices).map(|index| &mut self[index])
    }

    #[must_use]
    pub fn active_nde_filter<'a>(
        &self,
        selected_event_indices: &HashSet<EventIndex>,
        extradata: &'a Extradata,
    ) -> Option<&'a nde::Filter> {
        let event = self.active_event(selected_event_indices)?;
        extradata.nde_filter_for_event(event)
    }

    #[must_use]
    pub fn active_nde_filter_mut<'a>(
        &self,
        selected_event_indices: &HashSet<EventIndex>,
        extradata: &'a mut Extradata,
    ) -> Option<&'a mut nde::Filter> {
        let event = self.active_event(selected_event_indices)?;
        extradata.nde_filter_for_event_mut(event)
    }

    /// Dispatch message to node
    pub fn update_node(
        &mut self,
        selected_event_indices: &HashSet<EventIndex>,
        extradata: &mut Extradata,
        node_index: usize,
        message: message::Node,
    ) {
        if let Some(filter) = self.active_nde_filter_mut(selected_event_indices, extradata)
            && let Some(node) = filter.graph.nodes.get_mut(node_index)
        {
            node.node.update(message);
        }
    }

    /// Iterate over the event indices in the given range, in an undefined order.
    pub fn iter_range<'a>(
        &'a self,
        interval: &'a Range<StartTime>,
    ) -> impl Iterator<Item = EventIndex> + 'a {
        let iter = self
            .query_index
            .iter_overlaps(interval)
            .map(|(_, leaf)| leaf);

        LeafIterator {
            outer: iter,
            inner: None,
        }
    }

    /// Iterate over all event indices, sorted by start time.
    pub fn iter_all_time_sorted(&self) -> impl Iterator<Item = EventIndex> {
        let iter = self.query_index.iter().map(|(_, leaf)| leaf);

        LeafIterator {
            outer: iter,
            inner: None,
        }
    }

    /// Iterate over all event indices in logical order.
    pub fn iter_all_in_order(&self) -> impl Iterator<Item = EventIndex> {
        self.order.iter().copied()
    }

    /// Iterate over some range of event indices in logical order.
    pub fn iter_range_in_order(&self, range: Range<usize>) -> impl Iterator<Item = EventIndex> {
        self.order[range].iter().copied()
    }

    /// Iterate over all events immutably in an arbitrary order.
    pub fn iter_events(&self) -> impl Iterator<Item = &Event<'static>> {
        self.events.iter().flatten()
    }

    /// Iterate over all events mutably in an arbitrary order.
    pub fn iter_events_mut(&mut self) -> impl Iterator<Item = &mut Event<'static>> {
        self.events.iter_mut().flatten()
    }

    /// Compile subtitles in the given time range to ASS.
    #[must_use]
    pub fn compile_range<'a>(
        &'a self,
        extradata: &Extradata,
        context: &compile::Context,
        interval: Range<StartTime>,
    ) -> Vec<Event<'a>> {
        let mut compiled: Vec<Event<'a>> = vec![];

        for event_index in self.iter_range(&interval) {
            self.internal_compile_step(extradata, context, &mut compiled, event_index);
        }

        compiled
    }

    /// Compile all subtitles in this track to ASS.
    #[must_use]
    pub fn compile_all<'a>(
        &'a self,
        extradata: &Extradata,
        context: &compile::Context,
    ) -> Vec<Event<'a>> {
        let mut compiled: Vec<Event<'a>> = vec![];

        for event_index in self.iter_all_time_sorted() {
            self.internal_compile_step(extradata, context, &mut compiled, event_index);
        }

        compiled
    }

    fn internal_compile_step<'a>(
        &'a self,
        extradata: &Extradata,
        context: &compile::Context,
        compiled: &mut Vec<Event<'a>>,
        event_index: EventIndex,
    ) {
        let event = self.events[event_index.0].as_ref().unwrap();

        // Skip comments when compiling events
        if !event.is_comment() {
            // Run the complex `nde` compilation method if the event has a filter assigned,
            // and the trivial one otherwise
            match extradata.nde_filter_for_event(event) {
                Some(filter) => match compile::nde(event, &filter.graph, context) {
                    Ok(mut nde_result) => match &mut nde_result.events {
                        Some(events) => compiled.append(events),
                        None => println!("No output from NDE filter"),
                    },
                    Err(error) => {
                        println!("Got NdeError while running NDE filter: {error:?}");
                    }
                },
                None => compiled.push(compile::trivial(event)),
            }
        }
    }
}

impl Default for EventTrack {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl Debug for EventTrack {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let trail_s = if self.count == 1 { "" } else { "s" };
        write!(formatter, "EventTrack with {} event{trail_s}", self.count)
    }
}

impl FromIterator<Event<'static>> for EventTrack {
    fn from_iter<T: IntoIterator<Item = Event<'static>>>(iter: T) -> Self {
        let iterator = iter.into_iter();

        let mut track = Self {
            events: Vec::with_capacity(iterator.size_hint().0),
            query_index: interavl::IntervalTree::default(),
            order: indexmap::IndexSet::default(),
            count: 0,
        };

        for event in iterator {
            track.push(event);
        }

        track
    }
}

impl Index<EventIndex> for EventTrack {
    type Output = Event<'static>;

    fn index(&self, index: EventIndex) -> &Self::Output {
        self.events[index.0].as_ref().unwrap()
    }
}

impl IndexMut<EventIndex> for EventTrack {
    fn index_mut(&mut self, index: EventIndex) -> &mut Self::Output {
        self.events[index.0].as_mut().unwrap()
    }
}

enum Leaf {
    Single(EventIndex),
    Multiple(Vec<EventIndex>),
}

impl Leaf {
    fn count(&self) -> usize {
        match self {
            Leaf::Single(_) => 1,
            Leaf::Multiple(vec) => vec.len(),
        }
    }

    fn contains(&self, index: EventIndex) -> bool {
        match self {
            Leaf::Single(other) => *other == index,
            Leaf::Multiple(vec) => vec.contains(&index),
        }
    }
}

struct LeafIterator<'a, I: Iterator<Item = &'a Leaf> + 'a> {
    outer: I,
    inner: Option<<&'a [EventIndex] as IntoIterator>::IntoIter>,
}

impl<'a, I: Iterator<Item = &'a Leaf>> Iterator for LeafIterator<'a, I> {
    type Item = EventIndex;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(event_index) = self.inner.as_mut().and_then(Iterator::next) {
            Some(*event_index)
        } else if let Some(next_leaf) = self.outer.next() {
            match next_leaf {
                Leaf::Single(event_index) => Some(*event_index),
                Leaf::Multiple(vec) => {
                    let mut iter = vec.iter();
                    let item = *iter.next().unwrap();
                    self.inner = Some(iter);
                    Some(item)
                }
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_track_modify() {
        let events = vec![
            Event {
                start: StartTime(0),
                duration: Duration(1000),
                ..Event::default()
            },
            Event {
                start: StartTime(1000),
                duration: Duration(1000),
                ..Event::default()
            },
        ];

        let events_count = events.len();
        let mut track: EventTrack = events.into_iter().collect();
        assert!(!track.is_empty());
        assert_eq!(track.len(), events_count);

        assert_eq!(track.nth(0).1.start, StartTime(0));
        assert!(track.get_nth(2).is_none());

        track.push(Event {
            start: StartTime(3000),
            duration: Duration(1000),
            ..Event::default()
        });
        assert_eq!(track.nth(2).1.start, StartTime(3000));

        track.insert(
            track.nth(1).0,
            Event {
                start: StartTime(2000),
                duration: Duration(1000),
                ..Event::default()
            },
        );

        assert_eq!(track.nth(1).1.start, StartTime(2000));
        assert_eq!(track.nth(3).1.start, StartTime(3000));

        let mut to_remove = HashSet::from([track.nth(1).0, track.nth(2).0]);
        track.remove_from_set(&mut to_remove);
        assert_eq!(track.len(), 2);
        assert_eq!(track.nth(0).1.start, StartTime(0));
        assert_eq!(track.nth(1).1.start, StartTime(3000));
    }

    #[test]
    fn event_track_query() {
        let mut track = EventTrack::new_empty();
        assert!(track.is_empty());
        assert_eq!(track.events.len(), 0);
        assert_eq!(
            track.iter_range(&(StartTime(0)..StartTime(1000))).count(),
            0
        );

        track.push(Event {
            start: StartTime(1000),
            duration: Duration(1000),
            ..Event::default()
        });
        assert_eq!(
            track.iter_range(&(StartTime(0)..StartTime(1000))).count(),
            0
        );
        assert_eq!(
            track.iter_range(&(StartTime(500)..StartTime(1500))).count(),
            1
        );
        assert_eq!(
            track
                .iter_range(&(StartTime(1000)..StartTime(2000)))
                .count(),
            1
        );
        assert_eq!(
            track
                .iter_range(&(StartTime(1500)..StartTime(2500)))
                .count(),
            1
        );
        assert_eq!(
            track
                .iter_range(&(StartTime(2000)..StartTime(3000)))
                .count(),
            0
        );
        assert_eq!(track.iter_range(&(StartTime(0)..StartTime(0))).count(), 0);
        assert_eq!(track.iter_range(&(StartTime(1000).stab())).count(), 1);
        assert_eq!(track.iter_range(&(StartTime(1500).stab())).count(), 1);
        assert_eq!(track.iter_range(&(StartTime(2000).stab())).count(), 0);
        assert_eq!(track.iter_range(&(StartTime(3000).stab())).count(), 0);

        let event = Event {
            start: StartTime(3000),
            duration: Duration(1000),
            ..Event::default()
        };
        track.push(event.clone());
        track.push(event.clone());
        track.push(event.clone());

        let event = Event {
            start: StartTime(5000),
            duration: Duration(1000),
            ..Event::default()
        };
        track.push(event);

        assert_eq!(
            track
                .iter_range(&(StartTime(1000)..StartTime(6000)))
                .count(),
            5
        );
        assert_eq!(track.iter_range(&(StartTime(3000).stab())).count(), 3);
    }

    #[test]
    fn interavl_test_1() {
        let mut tree: interavl::IntervalTree<u64, ()> = interavl::IntervalTree::default();

        tree.insert(1..2, ());

        assert_eq!(tree.iter_overlaps(&(0..0)).count(), 0);
        assert_eq!(tree.iter_overlaps(&(0..1)).count(), 0);
        // assert_eq!(tree.iter_overlaps(&(1..1)).count(), 1); // fails, LHS == 0
        assert_eq!(tree.iter_overlaps(&(1..2)).count(), 1);
        assert_eq!(tree.iter_overlaps(&(2..2)).count(), 0);
        assert_eq!(tree.iter_overlaps(&(2..3)).count(), 0);
    }
}
