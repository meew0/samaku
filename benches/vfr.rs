use criterion::{BatchSize, Criterion};
use rand::prelude::*;
use samaku::{media, subtitle};

pub fn benchmark_vfr(c: &mut Criterion) {
    c.bench_function("construct CFR frame rate directly", |b| {
        b.iter(|| media::FrameRate::cfr(24000, 1001))
    });

    c.bench_function("construct CFR frame rate from 1000 frames", |b| {
        let timecodes = media::frame_util::cfr_timecodes(24000.0 / 1001.0, 1000);
        b.iter(|| {
            media::FrameRate::from_timecodes_iter(std::hint::black_box(timecodes.iter().copied()))
        })
    });

    c.bench_function("construct CFR frame rate from 1000000 frames", |b| {
        let timecodes = media::frame_util::cfr_timecodes(24000.0 / 1001.0, 1000000);
        b.iter(|| {
            media::FrameRate::from_timecodes_iter(std::hint::black_box(timecodes.iter().copied()))
        })
    });

    c.bench_function("construct VFR frame rate from 1000 frames", |b| {
        let timecodes = media::frame_util::vfr_timecodes(&[
            (24000.0 / 1001.0, 200),
            (30000.0 / 1001.0, 300),
            (24000.0 / 1001.0, 100),
            (60000.0 / 1001.0, 400),
        ]);
        b.iter(|| {
            media::FrameRate::from_timecodes_iter(std::hint::black_box(timecodes.iter().copied()))
        })
    });

    c.bench_function("construct VFR frame rate from 1000000 frames", |b| {
        let timecodes = media::frame_util::vfr_timecodes(&[
            (24000.0 / 1001.0, 200000),
            (30000.0 / 1001.0, 300000),
            (24000.0 / 1001.0, 100000),
            (60000.0 / 1001.0, 400000),
        ]);
        b.iter(|| {
            media::FrameRate::from_timecodes_iter(std::hint::black_box(timecodes.iter().copied()))
        })
    });

    c.bench_function("query CFR", |b| {
        let frame_rate = media::FrameRate::cfr(24000, 1001).unwrap();
        b.iter(|| {
            frame_rate.frame_at_time(
                std::hint::black_box(subtitle::StartTime(1000)),
                media::TimeMode::Exact,
            )
        })
    });

    c.bench_function("query VFR with 1000 frames", |b| {
        let timecodes = media::frame_util::vfr_timecodes(&[
            (24000.0 / 1001.0, 200),
            (30000.0 / 1001.0, 300),
            (24000.0 / 1001.0, 100),
            (60000.0 / 1001.0, 400),
        ]);
        let frame_rate = media::FrameRate::from_timecodes_iter(timecodes.iter().copied()).unwrap();
        let mut rng = rand_pcg::Mcg128Xsl64::new(1);
        b.iter_batched(
            || {
                subtitle::StartTime(
                    rng.random_range(
                        0..frame_rate
                            .time_at_frame(media::FrameNumber(1000), media::TimeMode::Exact)
                            .0,
                    ),
                )
            },
            |time| frame_rate.frame_at_time(time, media::TimeMode::Exact),
            BatchSize::SmallInput,
        )
    });

    c.bench_function("query VFR with 1000000 frames (fast implementation)", |b| {
        let timecodes = media::frame_util::vfr_timecodes(&[
            (24000.0 / 1001.0, 200000),
            (30000.0 / 1001.0, 300000),
            (24000.0 / 1001.0, 100000),
            (60000.0 / 1001.0, 400000),
        ]);
        let frame_rate = media::FrameRate::from_timecodes_iter(timecodes.iter().copied()).unwrap();
        let mut rng = rand_pcg::Mcg128Xsl64::new(1);
        b.iter_batched(
            || {
                subtitle::StartTime(
                    rng.random_range(
                        0..frame_rate
                            .time_at_frame(media::FrameNumber(1000000), media::TimeMode::Exact)
                            .0,
                    ),
                )
            },
            |time| frame_rate.frame_at_time(time, media::TimeMode::Exact),
            BatchSize::SmallInput,
        )
    });

    c.bench_function(
        "query VFR with 1000000 frames (reference implementation)",
        |b| {
            let timecodes = media::frame_util::vfr_timecodes(&[
                (24000.0 / 1001.0, 200000),
                (30000.0 / 1001.0, 300000),
                (24000.0 / 1001.0, 100000),
                (60000.0 / 1001.0, 400000),
            ]);
            let frame_rate =
                media::FrameRate::from_timecodes_iter(timecodes.iter().copied()).unwrap();
            let (numerator, denominator) = (frame_rate.numerator(), frame_rate.denominator());
            let mut rng = rand_pcg::Mcg128Xsl64::new(1);
            b.iter_batched(
                || {
                    rng.random_range(
                        0..frame_rate
                            .time_at_frame(media::FrameNumber(1000000), media::TimeMode::Exact)
                            .0,
                    )
                },
                |time| {
                    media::frame_util::ref_frame_at_exact(
                        timecodes.as_slice(),
                        numerator,
                        denominator,
                        time,
                    )
                },
                BatchSize::SmallInput,
            )
        },
    );
}
