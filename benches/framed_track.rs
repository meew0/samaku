//! Benchmarks for [`samaku::nde::FramedTrack`].
//!
//! The point of `FramedTrack` is to be faster than naively materializing and
//! cloning a `Vec` per frame in an NDE node graph, primarily by *moving* values
//! (which, for `nde::Event`, are expensive to clone) through the pipeline rather
//! than cloning them. These benchmarks measure the core `frame_zip*` operations
//! and contrast them against equivalent hand-written `Vec` implementations.
//!
//! Inputs are built fresh in each (untimed) `iter_batched` setup, so the timed
//! routine operates on uniquely-owned tracks and actually exercises the
//! move-based fast paths (which require a unique `Rc`).

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput};
use samaku::nde::FramedTrack;
use samaku::{media, nde, subtitle};

/// A realistic event body, so cloning an event is not free.
const EVENT_TEXT: &str = r"{\i1\c&HFF0000&\pos(100,200)}Sphinx of black quartz, judge my vow.";

/// Frame counts to sweep. Kept moderate because every batch rebuilds its inputs,
/// which clones the (heavy) source events; `Throughput` normalizes per element.
const SIZES: [i32; 2] = [256, 4096];

/// Number of values a `Stack` carries, i.e. how many outputs a one-to-many
/// expansion (such as the `Gradient` node) produces per input value.
const STACK_WIDTH: i32 = 16;

/// A fully-populated event whose clone cost is representative.
fn make_event(start: i32, duration: i32) -> nde::Event {
    let (global, spans) = nde::tags::parse(EVENT_TEXT);
    nde::Event {
        start: media::FrameNumber(start),
        duration: media::FrameDelta(duration),
        layer_index: 0,
        style_index: 0,
        margins: subtitle::Margins::default(),
        global_tags: *global,
        overrides: nde::tags::Local::empty(),
        text: spans,
    }
}

/// The `n` single-frame events with real content, as a plain `Vec`.
fn events_vec(start: i32, n: i32) -> Vec<nde::Event> {
    let template = make_event(start, n);
    (0..n)
        .map(|i| template.make_static(media::FrameNumber(start + i), media::FrameDelta(1)))
        .collect()
}

/// A `Fixed<i32>` track of per-frame values covering `[start, start + n)`,
/// standing in for e.g. a per-frame position track.
fn positions_track(start: i32, n: i32) -> FramedTrack<'static, i32> {
    FramedTrack::from_fixed(media::FrameNumber(start), 1, (start..start + n).collect())
}

/// A `Fixed<nde::Event>` track of `n` single-frame events with real content.
fn events_track(start: i32, n: i32) -> FramedTrack<'static, nde::Event> {
    FramedTrack::from_fixed(media::FrameNumber(start), 1, events_vec(start, n))
}

/// A `Variable<i32>` track with one value per frame over `[start, start + n)`.
fn positions_variable(start: i32, n: i32) -> FramedTrack<'static, i32> {
    FramedTrack::from_variable((0..n).map(|i| (media::FrameNumber(start + i), vec![start + i])))
}

/// A `Variable<nde::Event>` track with one event per frame over `[start, start + n)`.
fn events_variable(start: i32, n: i32) -> FramedTrack<'static, nde::Event> {
    let template = make_event(start, n);
    FramedTrack::from_variable((0..n).map(move |i| {
        let frame = media::FrameNumber(start + i);
        (
            frame,
            vec![template.make_static(frame, media::FrameDelta(1))],
        )
    }))
}

/// Splatting a single event over `n` frames against a per-frame value track.
///
/// This is the bread-and-butter NDE operation (one event, baked per frame). The
/// single event must be cloned per frame in both implementations, so this mainly
/// shows that `FramedTrack` adds no overhead over a hand-written loop while
/// reusing the `T2` track's allocation via its fast path.
pub fn benchmark_frame_zip_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("framed_track/bake single event");
    for &n in &SIZES {
        let elements = u64::try_from(n).unwrap();
        group.throughput(Throughput::Elements(elements));

        group.bench_with_input(BenchmarkId::new("framed_track", n), &n, |b, &n| {
            let event = make_event(0, n);
            b.iter_batched(
                || {
                    (
                        FramedTrack::from_single(event.clone()),
                        positions_track(0, n),
                    )
                },
                |(events, positions)| {
                    black_box(events.frame_zip(positions, |frame, mut ev, pos| {
                        ev.start = media::FrameNumber(frame.0 + pos.map_or(0, |p| *p));
                        ev
                    }))
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_with_input(BenchmarkId::new("vec_clone", n), &n, |b, &n| {
            let event = make_event(0, n);
            let positions: Vec<i32> = (0..n).collect();
            b.iter(|| {
                let mut out = Vec::with_capacity(usize::try_from(n).unwrap());
                for (i, &pos) in positions.iter().enumerate() {
                    let frame = media::FrameNumber(i32::try_from(i).unwrap());
                    let mut ev = event.clone();
                    ev.start = media::FrameNumber(frame.0 + pos);
                    out.push(ev);
                }
                black_box(out)
            });
        });
    }
    group.finish();
}

/// Zipping a `Fixed` track of events with a `Fixed` value track of matching
/// bounds: `FramedTrack`'s fast path, which *moves* every event through instead
/// of cloning it.
///
/// Contrasted against a `Vec` baseline that also moves (the best a `Vec` design
/// can do) and one that clones (what a naive pipeline stage, reading borrowed
/// source data, ends up doing). The takeaway: `FramedTrack` reaches move-speed
/// through its abstraction, well below the clone baseline for heavy events.
pub fn benchmark_frame_zip_fixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("framed_track/zip fixed events");
    for &n in &SIZES {
        let elements = u64::try_from(n).unwrap();
        group.throughput(Throughput::Elements(elements));

        group.bench_with_input(BenchmarkId::new("framed_track (move)", n), &n, |b, &n| {
            b.iter_batched(
                || (events_track(0, n), positions_track(0, n)),
                |(events, positions)| {
                    black_box(events.frame_zip(positions, |_frame, mut ev, pos| {
                        ev.start = media::FrameNumber(pos.map_or(0, |p| *p));
                        ev
                    }))
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_with_input(BenchmarkId::new("vec (move)", n), &n, |b, &n| {
            let positions: Vec<i32> = (0..n).collect();
            b.iter_batched(
                || events_vec(0, n),
                |events| {
                    black_box(
                        events
                            .into_iter()
                            .zip(&positions)
                            .map(|(mut ev, &pos)| {
                                ev.start = media::FrameNumber(pos);
                                ev
                            })
                            .collect::<Vec<_>>(),
                    )
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_with_input(BenchmarkId::new("vec (clone)", n), &n, |b, &n| {
            let events = events_vec(0, n);
            let positions: Vec<i32> = (0..n).collect();
            b.iter(|| {
                black_box(
                    events
                        .iter()
                        .zip(&positions)
                        .map(|(ev, &pos)| {
                            let mut cloned = ev.clone();
                            cloned.start = media::FrameNumber(pos);
                            cloned
                        })
                        .collect::<Vec<_>>(),
                )
            });
        });
    }
    group.finish();
}

/// The sliced fast path (`frame_zip_sliced_fixed_width`), which moves each
/// frame's events into a `SmallVec` and back without cloning.
pub fn benchmark_frame_zip_sliced(c: &mut Criterion) {
    let mut group = c.benchmark_group("framed_track/sliced fixed width");
    for &n in &SIZES {
        let elements = u64::try_from(n).unwrap();
        group.throughput(Throughput::Elements(elements));

        group.bench_with_input(BenchmarkId::new("framed_track", n), &n, |b, &n| {
            b.iter_batched(
                || (events_track(0, n), positions_track(0, n)),
                |(events, positions)| {
                    black_box(events.frame_zip_sliced_fixed_width(
                        positions,
                        |_frame, mut evs, pos| {
                            if let Some(ev) = evs.first_mut() {
                                ev.start = media::FrameNumber(pos.first().copied().unwrap_or(0));
                            }
                            evs
                        },
                    ))
                },
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// Expanding a single event into `STACK_WIDTH` values — the `Gradient` node
/// pattern, where one event becomes several covering the same frame range.
///
/// `expand` stores the closure's returned `Vec` directly as the `Stack`, so this
/// measures the unavoidable per-output clone plus the `Single` -> `Stack`
/// machinery, contrasted against a hand-written `Vec` build to show the
/// abstraction adds no overhead. The sweep `n` is the number of outputs.
pub fn benchmark_expand_stack(c: &mut Criterion) {
    let mut group = c.benchmark_group("framed_track/expand into stack");
    for &n in &SIZES {
        let elements = u64::try_from(n).unwrap();
        group.throughput(Throughput::Elements(elements));

        group.bench_with_input(BenchmarkId::new("framed_track", n), &n, |b, &n| {
            let event = make_event(0, 1);
            b.iter_batched(
                || FramedTrack::from_single(event.clone()),
                |events| {
                    black_box(events.expand(|ev| {
                        (0..n)
                            .map(|i| {
                                let mut cloned = ev.clone();
                                cloned.start = media::FrameNumber(i);
                                cloned
                            })
                            .collect()
                    }))
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_with_input(BenchmarkId::new("vec_clone", n), &n, |b, &n| {
            let event = make_event(0, 1);
            b.iter(|| {
                let mut out = Vec::with_capacity(usize::try_from(n).unwrap());
                for i in 0..n {
                    let mut cloned = event.clone();
                    cloned.start = media::FrameNumber(i);
                    out.push(cloned);
                }
                black_box(out)
            });
        });
    }
    group.finish();
}

/// Splatting a `Stack` of `STACK_WIDTH` events over a frame range against a
/// per-frame value track: the `frame_zip` `Stack` fast path. It borrows each
/// frame's `T2` value once and reuses it for all `STACK_WIDTH` stack entries,
/// cloning only the stack events (which must be cloned in any design).
pub fn benchmark_frame_zip_stack(c: &mut Criterion) {
    let mut group = c.benchmark_group("framed_track/splat stack");
    let stack_width = u64::try_from(STACK_WIDTH).unwrap();
    for &n in &SIZES {
        let elements = u64::try_from(n).unwrap() * stack_width;
        group.throughput(Throughput::Elements(elements));

        group.bench_with_input(BenchmarkId::new("framed_track", n), &n, |b, &n| {
            let event = make_event(0, n);
            b.iter_batched(
                || {
                    let stack: Vec<nde::Event> = (0..STACK_WIDTH).map(|_| event.clone()).collect();
                    (FramedTrack::from_stack(stack), positions_track(0, n))
                },
                |(events, positions)| {
                    black_box(events.frame_zip(positions, |frame, mut ev, pos| {
                        ev.start = media::FrameNumber(frame.0 + pos.map_or(0, |p| *p));
                        ev
                    }))
                },
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// `frame_zip_sliced_variable_width` with both tracks `Variable`. Contrast the
/// throughput against the `Fixed` sliced benchmark above (same map closure, same
/// per-frame data) to see the cost of the `BTreeMap`-backed representation:
/// ordered-map iteration plus a per-frame lookup into `track2`, versus the flat
/// contiguous `Vec` of the `Fixed` path.
pub fn benchmark_frame_zip_variable(c: &mut Criterion) {
    let mut group = c.benchmark_group("framed_track/sliced variable width");
    for &n in &SIZES {
        let elements = u64::try_from(n).unwrap();
        group.throughput(Throughput::Elements(elements));

        group.bench_with_input(BenchmarkId::new("framed_track", n), &n, |b, &n| {
            b.iter_batched(
                || (events_variable(0, n), positions_variable(0, n)),
                |(events, positions)| {
                    black_box(events.frame_zip_sliced_variable_width(
                        positions,
                        |_frame, mut evs, pos| {
                            if let Some(ev) = evs.first_mut() {
                                ev.start = media::FrameNumber(pos.first().copied().unwrap_or(0));
                            }
                            evs
                        },
                    ))
                },
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}
