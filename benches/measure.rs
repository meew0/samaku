use criterion::Criterion;
use samaku::{model, nde, subtitle};

fn test_event_and_style() -> (nde::Event, subtitle::Style) {
    let (global, spans) = nde::tags::parse(
        "{\\pos(0,0)\\an7\\b1\\i1\\fs160\\fsp5\\fnBarlow}Sphinx of black quartz,\\Njudge my vow",
    );
    let event = nde::Event {
        start: model::FrameNumber(0),
        duration: model::FrameDelta(24),
        layer_index: 0,
        style_index: 0,
        margins: subtitle::Margins::default(),
        global_tags: *global,
        overrides: nde::tags::Local::empty(),
        text: spans,
    };
    let style = subtitle::Style::default();
    (event, style)
}

pub fn benchmark_measure(c: &mut Criterion) {
    let (event, style) = test_event_and_style();

    c.bench_function("measure text extents", |b| {
        b.iter(|| {
            std::hint::black_box(nde::util::measure(
                std::hint::black_box(&event),
                std::hint::black_box(&style),
            ))
        })
    });
}
