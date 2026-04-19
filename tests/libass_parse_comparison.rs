//! Test our tag parsing and emitting code by comparing how libass behaves with subtitles that
//! went through a parse + emit cycle as compared to ones fed into it directly.

use std::borrow::Cow;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

use samaku::media;
use samaku::nde;
use samaku::nde::tags::{Colour, Transparency};
use samaku::subtitle;

pub const ASS_FILE: &str = include_str!("../test_files/parse_edge_cases.ass");

pub const FRAME_SIZE: subtitle::Resolution = subtitle::Resolution { x: 192, y: 108 };

// ignored on Windows since font rendering seems to be slightly non-deterministic,
// making it impossible to reliably compare rendered images.
// TODO: figure out whether maybe libass can be configured to be deterministic here
#[cfg_attr(windows, ignore)]
#[test]
fn libass_parse_comparison() {
    media::subtitle::set_libass_test_callback();

    let opaque_track = media::subtitle::OpaqueTrack::parse(&ASS_FILE.to_owned());
    let track = opaque_track.to_event_track();
    let styles = opaque_track.styles();

    let mut indirect_renderer = media::subtitle::Renderer::new();
    let mut direct_renderer = media::subtitle::Renderer::new();
    let build_hasher = RandomState::new();

    let mut found_any_difference = false;
    let mut count = 1;

    for event_index in track.iter_all_in_order() {
        let event = &track[event_index];
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

        let indirect = parse_round_trip(event);
        let indirect_opaque = media::subtitle::OpaqueTrack::from_compiled(
            std::iter::once(&indirect),
            &styles,
            &opaque_track.script_info(),
        );

        'inner: for now_offset in (0..event.duration.0).step_by(100) {
            let now = event.start.0 + now_offset;

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

            if direct_images != indirect_images {
                println!();
                println!("Found difference between direct and indirect image!");
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
                    for (i, (direct, indirect)) in
                        direct_images.iter().zip(indirect_images.iter()).enumerate()
                    {
                        if direct == indirect {
                            println!(" ({}) [equal]", i);
                        } else {
                            println!(" ({})", i);
                            println!("   Direct:   {:?}", direct);
                            println!("   Indirect: {:?}", indirect);

                            if direct.data != indirect.data {
                                let direct_path = write_bitmap(count, &direct.data.bytes, "direct");
                                let indirect_path =
                                    write_bitmap(count, &indirect.data.bytes, "indirect");
                                println!(
                                    "    -> wrote data files to {direct_path} and {indirect_path}"
                                );
                                count += 1;
                            }
                        }
                    }
                }

                found_any_difference = true;
                break 'inner;
            }
        }
    }

    assert!(!found_any_difference);
}

fn parse_round_trip(event: &subtitle::Event) -> subtitle::Event<'static> {
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

fn write_bitmap(count: u64, bitmap: &[u8], suffix: &str) -> String {
    use std::io::prelude::*;

    let path = format!("{count:04}_{suffix}.dat");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(bitmap).unwrap();
    path
}
