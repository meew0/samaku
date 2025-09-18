use crate::subtitle::{Duration, Event, Extradata, StartTime, compile};
use crate::{message, nde};
use std::collections::HashSet;
use std::fmt::Debug;
use std::ops::{Index, IndexMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EventIndex(usize);

/// Ordered collection of [`Event`]s.
/// For now, this is just a wrapper around [`Vec`], but in the future it might become more advanced,
/// using a tree-like structure or some time-indexed data structure.
#[derive(Clone, Default)]
pub struct EventTrack {
    events: Vec<Event<'static>>,
}

impl EventTrack {
    /// Create a new empty `EventTrack`.
    #[must_use]
    pub fn new_empty() -> Self {
        Self { events: vec![] }
    }

    /// Create a new `EventTrack` from the given `Vec` of events.
    #[must_use]
    pub fn from_vec(events: Vec<Event<'static>>) -> Self {
        Self { events }
    }

    /// Returns true if and only if there are no events in this track.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Returns the number of events in the track.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn get(&mut self, index: EventIndex) -> Option<&Event<'static>> {
        self.events.get(index.0)
    }

    #[must_use]
    pub fn get_mut(&mut self, index: EventIndex) -> Option<&mut Event<'static>> {
        self.events.get_mut(index.0)
    }

    #[must_use]
    pub fn nth(&self, n: usize) -> (EventIndex, &Event<'static>) {
        (EventIndex(n), &self.events[n])
    }

    pub fn update_event_times(&mut self, index: EventIndex, start: StartTime, duration: Duration) {
        let event = &mut self[index];
        event.start = start;
        event.duration = duration;
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Event<'static>] {
        self.events.as_slice()
    }

    pub fn push(&mut self, event: Event<'static>) {
        self.events.push(event);
    }

    pub fn insert(&mut self, index: EventIndex, event: Event<'static>) {
        self.events.insert(index.0, event);
    }

    /// Remove all events whose indices are contained in the given set. Clears the set afterwards
    /// (since the indices it references are no longer valid); hence, it requires a mutable
    /// reference to the set.
    pub fn remove_from_set(&mut self, set: &mut HashSet<EventIndex>) {
        let mut index = 0;
        self.events.retain(|_| {
            let to_remove = set.contains(&EventIndex(index));
            index += 1;
            !to_remove
        });
        set.clear();
    }

    /// If exactly one event is selected, this method returns the index of that element. Otherwise,
    /// it returns `None`.
    fn active_event_index(selected_event_indices: &HashSet<EventIndex>) -> Option<EventIndex> {
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

    /// Iterate over the events in the given range.
    pub fn iter_range(
        &'_ self,
        start: StartTime,
        end: StartTime,
    ) -> impl Iterator<Item = (EventIndex, &Event<'_>)> {
        // TODO: make this more efficient using an interval tree or the like
        self.events
            .iter()
            .enumerate()
            .filter(move |(_, event)| (event.start + event.duration) > start && event.start < end)
            .map(|(index, event)| (EventIndex(index), event))
    }

    /// Iterate over the events at the given time point.
    pub fn iter_stab(&'_ self, time: StartTime) -> impl Iterator<Item = (EventIndex, &Event<'_>)> {
        self.events
            .iter()
            .enumerate()
            .filter(move |(_, event)| (event.start + event.duration) > time && event.start <= time)
            .map(|(index, event)| (EventIndex(index), event))
    }

    /// Compile subtitles in the given frame range to ASS.
    #[must_use]
    pub fn compile<'a>(
        &'a self,
        extradata: &Extradata,
        context: &compile::Context,
        _frame_start: i32,
        _frame_count: Option<i32>,
    ) -> Vec<Event<'a>> {
        let mut compiled: Vec<Event<'a>> = vec![];

        for event in &self.events {
            // Skip comments when compiling events
            if event.is_comment() {
                continue;
            }

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

        compiled
    }
}

impl Debug for EventTrack {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let trail_s = if self.events.len() == 1 { "" } else { "s" };
        write!(
            formatter,
            "EventTrack with {} event{trail_s}",
            self.events.len()
        )
    }
}

// For now, just transparently pass along `Vec`'s implementation
#[expect(
    clippy::into_iter_without_iter,
    reason = "iter on underlying method not needed at this time"
)]
impl<'a> IntoIterator for &'a EventTrack {
    type Item = &'a Event<'static>;
    type IntoIter = <&'a Vec<Event<'static>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        <&'a Vec<Event<'static>> as IntoIterator>::into_iter(&self.events)
    }
}

#[expect(
    clippy::into_iter_without_iter,
    reason = "iter on underlying method not needed at this time"
)]
impl<'a> IntoIterator for &'a mut EventTrack {
    type Item = &'a mut Event<'static>;
    type IntoIter = <&'a mut Vec<Event<'static>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        <&'a mut Vec<Event<'static>> as IntoIterator>::into_iter(&mut self.events)
    }
}

impl Index<EventIndex> for EventTrack {
    type Output = Event<'static>;

    fn index(&self, index: EventIndex) -> &Self::Output {
        &self.events[index.0]
    }
}

impl IndexMut<EventIndex> for EventTrack {
    fn index_mut(&mut self, index: EventIndex) -> &mut Self::Output {
        &mut self.events[index.0]
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

        let mut track = EventTrack::from_vec(events.clone());
        assert!(!track.is_empty());
        assert_eq!(track.len(), events.len());

        assert_eq!(track[EventIndex(0)].start, StartTime(0));
        assert!(track.get(EventIndex(2)).is_none());

        track.push(Event {
            start: StartTime(3000),
            duration: Duration(1000),
            ..Event::default()
        });
        assert_eq!(track[EventIndex(2)].start, StartTime(3000));

        track.insert(
            EventIndex(1),
            Event {
                start: StartTime(2000),
                duration: Duration(1000),
                ..Event::default()
            },
        );
        assert_eq!(track[EventIndex(1)].start, StartTime(2000));
        assert_eq!(track[EventIndex(3)].start, StartTime(3000));

        let mut to_remove = HashSet::from([EventIndex(1), EventIndex(2)]);
        track.remove_from_set(&mut to_remove);
        assert_eq!(track.len(), 2);
    }

    #[test]
    fn event_track_query() {
        let mut track = EventTrack::new_empty();
        assert!(track.is_empty());
        assert_eq!(track.events.len(), 0);
        assert_eq!(track.iter_range(StartTime(0), StartTime(1000)).count(), 0);

        track.push(Event {
            start: StartTime(1000),
            duration: Duration(1000),
            ..Event::default()
        });
        assert_eq!(track.iter_range(StartTime(0), StartTime(1000)).count(), 0);
        assert_eq!(track.iter_range(StartTime(500), StartTime(1500)).count(), 1);
        assert_eq!(
            track.iter_range(StartTime(1000), StartTime(2000)).count(),
            1
        );
        assert_eq!(
            track.iter_range(StartTime(1500), StartTime(2500)).count(),
            1
        );
        assert_eq!(
            track.iter_range(StartTime(2000), StartTime(3000)).count(),
            0
        );
        assert_eq!(track.iter_stab(StartTime(0)).count(), 0);
        assert_eq!(track.iter_stab(StartTime(1000)).count(), 1);
        assert_eq!(track.iter_stab(StartTime(1500)).count(), 1);
        assert_eq!(track.iter_stab(StartTime(2000)).count(), 0);
        assert_eq!(track.iter_stab(StartTime(3000)).count(), 0);
    }
}
