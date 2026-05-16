//! Test our tag parsing, baking, and emitting code by comparing how libass behaves with subtitles that
//! went through a parse + (bake +) emit cycle as compared to ones fed into it directly.

use std::borrow::Cow;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

use samaku::media;
use samaku::nde;
use samaku::nde::tags::{Colour, Transparency};
use samaku::subtitle;

pub const ASS_FILES: &[(&str, &str)] = &[
    (
        "parse_edge_cases.ass",
        include_str!("../test_files/parse_edge_cases.ass"),
    ),
    (
        "bake_edge_cases.ass",
        include_str!("../test_files/bake_edge_cases.ass"),
    ),
];

pub const FRAME_SIZE: subtitle::Resolution = subtitle::Resolution { x: 192, y: 108 };

// ignored on Windows since font rendering seems to be slightly non-deterministic,
// making it impossible to reliably compare rendered images.
// TODO: figure out whether maybe libass can be configured to be deterministic here
#[cfg_attr(windows, ignore)]
#[test]
fn parse() {
    media::subtitle::set_libass_test_callback();

    for &(file_name, file_content) in ASS_FILES {
        run_comparison(file_name, file_content, "parse", parse_round_trip)
    }
}

#[cfg_attr(windows, ignore)]
#[test]
fn bake() {
    media::subtitle::set_libass_test_callback();

    for &(file_name, file_content) in ASS_FILES {
        run_comparison(file_name, file_content, "bake", bake_round_trip)
    }
}

fn run_comparison<
    F: Fn(
        nde::tags::bake::TimeContext,
        &subtitle::Event,
        &[subtitle::Style],
        subtitle::Resolution,
    ) -> subtitle::Event<'static>,
>(
    file_name: &str,
    file_content: &str,
    test_name: &str,
    round_trip_fn: F,
) {
    let opaque_track = media::subtitle::OpaqueTrack::parse(file_content);
    let track = opaque_track.to_event_track();
    let styles = opaque_track.styles();
    let playback_resolution = opaque_track.script_info().playback_resolution;

    let mut indirect_renderer = media::subtitle::Renderer::new();
    let mut direct_renderer = media::subtitle::Renderer::new();
    let build_hasher = RandomState::new();

    let mut failed = false;
    let mut count = 1;

    for event_index in track.iter_all_in_order() {
        let event = &track[event_index];
        let should_fail = event
            .effect
            .strip_prefix("fail:")
            .and_then(|s| s.split(',').find(|name| *name == test_name))
            .is_some();
        let mut failed_once = false;

        let direct = subtitle::Event {
            start: event.start,
            duration: event.duration,
            layer_index: event.layer_index,
            style_index: event.style_index,
            margins: event.margins,
            text: Cow::Borrowed(&event.text),
            ..Default::default()
        };
        let direct_opaque = media::subtitle::OpaqueTrack::from_compiled(
            std::iter::once(&direct),
            &styles,
            &opaque_track.script_info(),
        );

        'inner: for now_offset in (0..event.duration.0).step_by(100) {
            let now = event.start.0 + now_offset;

            let time = nde::tags::bake::TimeContext {
                start: event.start,
                duration: event.duration,
                now: subtitle::StartTime(now),
            };

            let indirect = round_trip_fn(time, event, &styles, playback_resolution);
            let indirect_opaque = media::subtitle::OpaqueTrack::from_compiled(
                std::iter::once(&indirect),
                &styles,
                &opaque_track.script_info(),
            );

            let mut direct_images: Vec<AssImage> = vec![];
            let mut indirect_images: Vec<AssImage> = vec![];

            direct_renderer.render_subtitles_with_callback(
                &direct_opaque,
                now,
                FRAME_SIZE,
                FRAME_SIZE,
                &mut |image| direct_images.push(AssImage::from(image, &build_hasher)),
            );

            indirect_renderer.render_subtitles_with_callback(
                &indirect_opaque,
                now,
                FRAME_SIZE,
                FRAME_SIZE,
                &mut |image| indirect_images.push(AssImage::from(image, &build_hasher)),
            );

            let different = direct_images.len() != indirect_images.len()
                || direct_images
                    .iter()
                    .zip(indirect_images.iter())
                    .any(|(direct, indirect)| !direct.visible_eq(indirect));

            if different {
                failed_once = true;

                if !should_fail {
                    println!();
                    println!("Found difference between direct and indirect image!");
                    println!(" - File: {file_name}");
                    println!(" - Direct text:   {}", direct.text);
                    println!(" - Indirect text: {}", indirect.text);
                    println!(
                        " - At time point: {} ms from start time ({} ms)",
                        now_offset, event.start.0
                    );
                    if direct_images.len() != indirect_images.len() {
                        println!(
                            " ! Different number of images: {} direct vs {} indirect",
                            direct_images.len(),
                            indirect_images.len()
                        );
                    } else {
                        println!(" : Found {} images", direct_images.len());
                        for (i, (direct_img, indirect_img)) in
                            direct_images.iter().zip(indirect_images.iter()).enumerate()
                        {
                            if direct_img == indirect_img {
                                println!(" ({}) [equal]", i);
                            } else {
                                println!(" ({})", i);
                                println!("   Direct:   {:?}", direct_img);
                                println!("   Indirect: {:?}", indirect_img);
                            }
                        }
                    }

                    let path = write_all_bitmaps(
                        count,
                        &direct_images,
                        &indirect_images,
                        &direct.text,
                        &indirect.text,
                        now_offset,
                        event.start.0,
                        FRAME_SIZE.x,
                        FRAME_SIZE.y,
                    );
                    println!("    -> wrote comparison file to {path}");
                    count += 1;

                    break 'inner;
                }
            }
        }

        if failed_once ^ should_fail {
            failed = true;

            if should_fail {
                println!();
                println!(
                    "Found no difference between direct and indirect image, even though one was expected!"
                );
                println!(" - File: {file_name}");
                println!(" - Direct text:   {}", direct.text);
            }
        }
    }

    assert!(!failed);
}

fn parse_round_trip(
    _time: nde::tags::bake::TimeContext,
    event: &subtitle::Event,
    _styles: &[subtitle::Style],
    _playback_resolution: subtitle::Resolution,
) -> subtitle::Event<'static> {
    let (global, spans) = nde::tags::parse(&event.text);
    let emitted = nde::tags::emit(&global, &spans);

    subtitle::Event {
        start: event.start,
        duration: event.duration,
        layer_index: event.layer_index,
        style_index: event.style_index,
        margins: event.margins,
        text: Cow::Owned(emitted),
        ..Default::default()
    }
}

fn bake_round_trip(
    time: nde::tags::bake::TimeContext,
    event: &subtitle::Event,
    styles: &[subtitle::Style],
    playback_resolution: subtitle::Resolution,
) -> subtitle::Event<'static> {
    let (mut global, mut spans) = nde::tags::parse(&event.text);

    let event_style = &styles[event.style_index];
    let style_lookup = |name: &str| styles.iter().find(|style| style.name == name);

    nde::tags::bake(
        time,
        event_style,
        &style_lookup,
        &mut global,
        &mut spans,
        playback_resolution,
        None,
    );

    let emitted = nde::tags::emit(&global, &spans);

    subtitle::Event {
        start: event.start,
        duration: event.duration,
        layer_index: event.layer_index,
        style_index: event.style_index,
        margins: event.margins,
        text: Cow::Owned(emitted),
        ..Default::default()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct AssImage {
    width: i32,
    height: i32,
    dest_x: i32,
    dest_y: i32,
    stride: i32,
    colour: Colour,
    transparency: Transparency,
    data: ImageData,
}

impl AssImage {
    pub fn from<BH: BuildHasher>(image: &media::subtitle::Image, build_hasher: &BH) -> Self {
        let (colour, transparency) =
            subtitle::unpack_colour_and_transparency_rgbt(image.metadata.color);

        Self {
            width: image.metadata.w,
            height: image.metadata.h,
            dest_x: image.metadata.dst_x,
            dest_y: image.metadata.dst_y,
            stride: image.metadata.stride,
            colour,
            transparency,
            data: ImageData::new(image.bitmap, build_hasher),
        }
    }

    pub fn visible_eq(&self, other: &AssImage) -> bool {
        if self.transparency == Transparency(255) && other.transparency == Transparency(255) {
            // Do not compare the image data
            self.width == other.width
                && self.height == other.height
                && self.dest_x == other.dest_x
                && self.dest_y == other.dest_y
                && self.stride == other.stride
        } else {
            self == other
        }
    }
}

#[derive(PartialEq, Eq)]
struct ImageData {
    bytes: Vec<u8>,
    hash: u64,
}

impl ImageData {
    pub fn new<BH: BuildHasher>(bitmap: &[u8], build_hasher: &BH) -> Self {
        let hash = build_hasher.hash_one(bitmap);

        Self {
            bytes: bitmap.to_owned(),
            hash,
        }
    }
}

impl std::fmt::Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageData")
            .field("hash", &self.hash)
            .finish()
    }
}

#[expect(clippy::too_many_arguments, reason = "it's simpler like this")]
fn write_all_bitmaps(
    count: u64,
    direct_images: &[AssImage],
    indirect_images: &[AssImage],
    direct_text: &str,
    indirect_text: &str,
    now_offset: i64,
    start_time: i64,
    frame_width: i32,
    frame_height: i32,
) -> String {
    use std::io::prelude::*;

    fn json_escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out
    }

    fn encode_image(image: &AssImage) -> String {
        let bitmap_b64 = data_encoding::BASE64.encode(&image.data.bytes);
        format!(
            r#"{{"width":{w},"height":{h},"stride":{s},"dest_x":{dx},"dest_y":{dy},"colour_red":{cr},"colour_green":{cg},"colour_blue":{cb},"transparency":{t},"bitmap":"{bmp}"}}"#,
            w = image.width,
            h = image.height,
            s = image.stride,
            dx = image.dest_x,
            dy = image.dest_y,
            cr = image.colour.red,
            cg = image.colour.green,
            cb = image.colour.blue,
            t = image.transparency.0,
            bmp = bitmap_b64,
        )
    }

    fn encode_images(images: &[AssImage]) -> String {
        let parts: Vec<String> = images.iter().map(encode_image).collect();
        format!("[{}]", parts.join(","))
    }

    let json = format!(
        r#"{{"frame_width":{fw},"frame_height":{fh},"direct_text":"{dt}","indirect_text":"{it}","now_offset_ms":{no},"start_time_ms":{st},"direct_images":{di},"indirect_images":{ii}}}"#,
        fw = frame_width,
        fh = frame_height,
        dt = json_escape(direct_text),
        it = json_escape(indirect_text),
        no = now_offset,
        st = start_time,
        di = encode_images(direct_images),
        ii = encode_images(indirect_images),
    );

    std::fs::create_dir_all("test_outputs/libass_compare").unwrap();
    let path = format!("test_outputs/libass_compare/{count:04}.json");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(json.as_bytes()).unwrap();
    path
}
