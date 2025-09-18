use std::hint::black_box;

use criterion::Criterion;

pub fn benchmark_parse(c: &mut Criterion) {
    const NO_TAGS: &str = "Sphinx of black quartz, judge my vow.";
    const FEW_TAGS: &str = r"{\i1\c&HFF0000&}Sphinx of black quartz, judge my vow.";
    const MANY_TAGS: &str = r"{\xbord1\ybord2\xshad3\yshad4\fax5\fay6\clip(70,80,90,100)\iclip(20,20,30,30)\iclip(1,m 0 0 s 20 0 20 20 0 20 c)\clip(2,m 0 0 s 20 0 20 20 0 20 c)\blur11\fscx12\fscy13\fsp14\fs15\frx16\fry17\frz18\fnAlegreya\an5\pos(19,20)\fade(0,255,0,0,1000,2000,3000)\org(21,22)\t(\xbord23)\1c&HFF0000&\2c&H00FF00&\3c&H0000FF&\4c&HFF00FF&\1a&H22&\2a&H44&\3a&H66&\4a&H88&\be24\b1\i1\kt25\s1\u1\pbo26\q1\fe1}All tags 1{\p1}m 0 0 s 100 0 100 100 0 100 c{\p0}";
    const MANY_SPANS: &str = r"{\xbord1}some text {\ybord2}some text {\xshad3}some text {\yshad4}some text {\fax5}some text {\fay6}some text {\clip(70,80,90,100)}some text {\iclip(20,20,30,30)}some text {\iclip(1,m 0 0 s 20 0 20 20 0 20 c)}some text {\clip(2,m 0 0 s 20 0 20 20 0 20 c)}some text {\blur11}some text {\fscx12}some text {\fscy13}some text {\fsp14}some text {\fs15}some text {\frx16}some text {\fry17}some text {\frz18}some text {\fnAlegreya}some text {\an5}some text {\pos(19,20)}some text {\fade(0,255,0,0,1000,2000,3000)}some text {\org(21,22)}aaa {\t(\xbord23)}bbb {\1c&HFF0000&}ccc {\2c&H00FF00&}xyz {\3c&H0000FF&}xyz {\4c&HFF00FF&}xyz {\1a&H22&}xyz {\2a&H44&}xyz {\3a&H66&}xyz {\4a&H88&}xyz {\be24}xyz {\b1}xyz {\i1}xyz {\kt25}xyz {\s1}xyz {\u1}xyz {\pbo26}xyz {\q1}xyz {\fe1}All tags 1{\p1}m 0 0 s 100 0 100 100 0 100 c{\p0}";

    c.bench_function("no tags", |b| {
        b.iter(|| samaku::nde::tags::parse(black_box(NO_TAGS)))
    });
    c.bench_function("few tags", |b| {
        b.iter(|| samaku::nde::tags::parse(black_box(FEW_TAGS)))
    });
    c.bench_function("many tags", |b| {
        b.iter(|| samaku::nde::tags::parse(black_box(MANY_TAGS)))
    });
    c.bench_function("many spans", |b| {
        b.iter(|| samaku::nde::tags::parse(black_box(MANY_SPANS)))
    });
}
