mod event_track;
mod nde;

use criterion::{criterion_group, criterion_main};

criterion_group!(nde, nde::benchmark_parse);
criterion_group!(
    event_track,
    event_track::benchmark_create,
    event_track::benchmark_insert,
    event_track::benchmark_remove,
    event_track::benchmark_query,
    event_track::benchmark_update,
);
criterion_main!(event_track, nde);
