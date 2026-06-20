mod event_track;
mod framed_track;
mod measure;
mod nde;
mod vfr;

use criterion::{criterion_group, criterion_main};

criterion_group!(nde, nde::benchmark_parse, nde::benchmark_bake);
criterion_group!(
    framed_track,
    framed_track::benchmark_frame_zip_single,
    framed_track::benchmark_frame_zip_fixed,
    framed_track::benchmark_frame_zip_sliced,
    framed_track::benchmark_frame_zip_variable,
);
criterion_group!(
    event_track,
    event_track::benchmark_create,
    event_track::benchmark_insert,
    event_track::benchmark_remove,
    event_track::benchmark_query,
    event_track::benchmark_update,
);
criterion_group!(measure, measure::benchmark_measure);
criterion_group!(vfr, vfr::benchmark_vfr);
criterion_main!(event_track, framed_track, measure, nde, vfr);
