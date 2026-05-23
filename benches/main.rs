mod event_track;
mod measure;
mod nde;

use criterion::{criterion_group, criterion_main};

criterion_group!(nde, nde::benchmark_parse, nde::benchmark_bake);
criterion_group!(
    event_track,
    event_track::benchmark_create,
    event_track::benchmark_insert,
    event_track::benchmark_remove,
    event_track::benchmark_query,
    event_track::benchmark_update,
);
criterion_group!(measure, measure::benchmark_measure);
criterion_main!(event_track, measure, nde);
