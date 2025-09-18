use criterion::{BatchSize, Criterion};
use rand::prelude::*;
use samaku::subtitle::{Duration, Event, EventIndex, EventTrack, EventType, Margins, StartTime};
use std::borrow::Cow;
use std::collections::HashSet;
use std::hint::black_box;
use std::iter;

const EMPTY_COW_STR: Cow<'static, str> = Cow::Borrowed("");

struct EventSource {
    pub count: usize,
    max_start_time: StartTime,
    max_duration: Duration,
    rng: rand_pcg::Mcg128Xsl64,
}

impl EventSource {
    fn new(count: usize) -> Self {
        let max_time = StartTime(count as i64 * 1000);
        let max_duration = Duration(20.min(count as i64 / 2) * 1000);

        Self {
            rng: rand_pcg::Mcg128Xsl64::new(1),
            count,
            max_start_time: max_time - max_duration,
            max_duration,
        }
    }

    fn next_event(&mut self) -> Event<'static> {
        Event {
            start: StartTime(self.rng.random_range(0..self.max_start_time.0)),
            duration: Duration(self.rng.random_range(0..self.max_duration.0)),
            layer_index: 0,
            style_index: 0,
            margins: Margins::default(),
            text: EMPTY_COW_STR,
            actor: EMPTY_COW_STR,
            effect: EMPTY_COW_STR,
            event_type: EventType::Dialogue,
            extradata_ids: vec![],
        }
    }

    fn collect(&mut self) -> Vec<Event<'static>> {
        let count = self.count;
        iter::repeat_with(|| self.next_event())
            .take(count)
            .collect()
    }

    fn random_start_time(&mut self) -> StartTime {
        StartTime(self.rng.random_range(0..self.max_start_time.0))
    }

    fn random_duration(&mut self) -> Duration {
        Duration(self.rng.random_range(0..self.max_duration.0))
    }

    fn random_index(&mut self) -> EventIndex {
        EventIndex(self.rng.random_range(0..self.count))
    }
}

pub fn benchmark_create(c: &mut Criterion) {
    fn to_bench(events_slice: &[Event<'static>]) {
        let count = events_slice.len();
        let events_vec = events_slice.to_owned(); // we explicitly want to time this as well
        let track = EventTrack::from_vec(events_vec);
        assert_eq!(track.len(), count);
    }

    fn perform_bench(c: &mut Criterion, count: usize) {
        c.bench_function(format!("create from slice: {count} events").as_str(), |b| {
            let data = EventSource::new(count).collect();
            b.iter(|| to_bench(data.as_slice()));
        });
    }

    perform_bench(c, 100);
    perform_bench(c, 1000);
    perform_bench(c, 10000);
}

pub fn benchmark_insert(c: &mut Criterion) {
    c.bench_function("insert 100 events at end into fresh track", |b| {
        b.iter_batched_ref(
            || EventSource::new(100).collect(),
            |events| {
                let count = events.len();
                let mut track = EventTrack::new_empty();
                for event in events {
                    track.push(event.clone());
                }
                assert_eq!(track.len(), count);
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("insert 100 events at end into track with 10k events", |b| {
        b.iter_batched_ref(
            || {
                let mut source = EventSource::new(10000);
                let track = EventTrack::from_vec(source.collect());
                let additional_count = 100;
                let additional_events: Vec<_> =
                    (0..additional_count).map(|_| source.next_event()).collect();
                (track, additional_events)
            },
            |(track, additional_events)| {
                let original_count = track.len();
                let additional_count = additional_events.len();
                for event in additional_events {
                    track.push(event.clone());
                }
                assert_eq!(track.len(), original_count + additional_count);
            },
            BatchSize::LargeInput,
        )
    });

    c.bench_function(
        "insert 100 events in the middle into track with 10k events",
        |b| {
            b.iter_batched_ref(
                || {
                    let mut source = EventSource::new(10000);
                    let track = EventTrack::from_vec(source.collect());
                    let additional_count = 100;
                    let step = source.count / additional_count;
                    let additional_events: Vec<_> = (0..additional_count)
                        .map(|i| (EventIndex(i * step), source.next_event()))
                        .collect();
                    (track, additional_events)
                },
                |(track, additional_events)| {
                    let original_count = track.len();
                    let additional_count = additional_events.len();
                    for (index, event) in additional_events {
                        track.insert(*index, event.clone());
                    }
                    assert_eq!(track.len(), original_count + additional_count);
                },
                BatchSize::LargeInput,
            )
        },
    );
}

pub fn benchmark_remove(c: &mut Criterion) {
    c.bench_function(
        "remove single event at end from track with 10k events",
        |b| {
            b.iter_batched_ref(
                || {
                    let track = EventTrack::from_vec(EventSource::new(10000).collect());
                    let remove_set = HashSet::from([EventIndex(9999)]);
                    (track, remove_set)
                },
                |(track, remove_set)| track.remove_from_set(remove_set),
                BatchSize::LargeInput,
            )
        },
    );

    c.bench_function(
        "remove single event at middle from track with 10k events",
        |b| {
            b.iter_batched_ref(
                || {
                    let track = EventTrack::from_vec(EventSource::new(10000).collect());
                    let remove_set = HashSet::from([EventIndex(5000)]);
                    (track, remove_set)
                },
                |(track, remove_set)| track.remove_from_set(remove_set),
                BatchSize::LargeInput,
            )
        },
    );

    c.bench_function(
        "remove 100 events at middle from track with 10k events",
        |b| {
            b.iter_batched_ref(
                || {
                    let track = EventTrack::from_vec(EventSource::new(10000).collect());
                    let remove_set = HashSet::from_iter((0..10000).step_by(100).map(EventIndex));
                    (track, remove_set)
                },
                |(track, remove_set)| track.remove_from_set(remove_set),
                BatchSize::LargeInput,
            )
        },
    );
}

pub fn benchmark_query(c: &mut Criterion) {
    fn perform_bench_range(c: &mut Criterion, count: usize) {
        c.bench_function(
            format!("query range (10 seconds) with {count} events").as_str(),
            |b| {
                let mut source = EventSource::new(count);
                let track = EventTrack::from_vec(source.collect());
                b.iter_batched(
                    || {
                        let start = source.random_start_time();
                        let end = start + Duration(10000);
                        (start, end)
                    },
                    |(start, end)| {
                        for (_, event) in track.iter_range(start, end) {
                            black_box(event);
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    perform_bench_range(c, 100);
    perform_bench_range(c, 10000);
    perform_bench_range(c, 1000000);

    fn perform_bench_stab(c: &mut Criterion, count: usize) {
        c.bench_function(format!("query stab with {count} events").as_str(), |b| {
            let mut source = EventSource::new(count);
            let track = EventTrack::from_vec(source.collect());
            b.iter_batched(
                || source.random_start_time(),
                |time| {
                    for (_, event) in track.iter_stab(time) {
                        black_box(event);
                    }
                },
                BatchSize::SmallInput,
            )
        });
    }

    perform_bench_stab(c, 100);
    perform_bench_stab(c, 10000);
    perform_bench_stab(c, 1000000);
}

pub fn benchmark_update(c: &mut Criterion) {
    c.bench_function("update event time randomly with 10k events", |b| {
        b.iter_batched_ref(
            || {
                let mut source = EventSource::new(10000);
                let track = black_box(EventTrack::from_vec(source.collect()));
                let index = source.random_index();
                let new_start = source.random_start_time();
                let new_duration = source.random_duration();
                (track, index, new_start, new_duration)
            },
            |(track, index, new_start, new_duration)| {
                track.update_event_times(*index, *new_start, *new_duration);
            },
            BatchSize::LargeInput,
        )
    });

    fn setup() -> (EventTrack, EventIndex) {
        let mut source = EventSource::new(10000);
        let track = black_box(EventTrack::from_vec(source.collect()));
        let index = source.random_index();
        (track, index)
    }

    c.bench_function("shift start time with 10k events", |b| {
        b.iter_batched_ref(
            setup,
            |(track, index)| {
                let event = &mut track[*index];
                let new_start = event.start + Duration(100);
                let new_duration = Duration(event.duration.0 - 100);
                track.update_event_times(*index, new_start, new_duration);
            },
            BatchSize::LargeInput,
        )
    });
    c.bench_function("shift end time with 10k events", |b| {
        b.iter_batched_ref(
            setup,
            |(track, index)| {
                let event = &mut track[*index];
                let new_start = event.start;
                let new_duration = Duration(event.duration.0 - 100);
                track.update_event_times(*index, new_start, new_duration);
            },
            BatchSize::LargeInput,
        )
    });
    c.bench_function("shift entire event with 10k events", |b| {
        b.iter_batched_ref(
            setup,
            |(track, index)| {
                let event = &mut track[*index];
                let new_start = event.start + Duration(100);
                let new_duration = event.duration;
                track.update_event_times(*index, new_start, new_duration);
            },
            BatchSize::LargeInput,
        )
    });
}
