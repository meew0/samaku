//! Test our tag parsing and emitting code by comparing how libass behaves with subtitles that
//! went through a parse + emit cycle as compared to ones fed into it directly.

use std::borrow::Cow;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

use samaku::media;
use samaku::nde;
use samaku::subtitle;

pub const ASS_FILE: &str = include_str!("../test_files/parse_edge_cases.ass");

pub const FRAME_SIZE: subtitle::Resolution = subtitle::Resolution { x: 192, y: 108 };

#[test]
fn libass_parse_comparison() {
    let opaque_track = media::subtitle::OpaqueTrack::parse(&ASS_FILE.to_owned());
    let track = opaque_track.to_sline_track();

    let mut indirect_renderer = media::subtitle::Renderer::new();
    let mut direct_renderer = media::subtitle::Renderer::new();
    let build_hasher = RandomState::new();

    let mut found_any_difference = false;

    for sline in track.slines.iter() {
        let direct = subtitle::ass::Event {
            start: sline.start,
            duration: sline.duration,
            layer_index: sline.layer_index,
            style_index: sline.style_index,
            margins: sline.margins,
            text: Cow::Borrowed(&sline.text),
            read_order: 0,
            name: Cow::Borrowed(""),
            effect: Cow::Borrowed(""),
        };
        let direct_opaque = media::subtitle::OpaqueTrack::from_compiled(
            std::iter::once(&direct),
            &track.styles,
            &opaque_track.script_info(),
        );

        let indirect = parse_round_trip(sline);
        let indirect_opaque = media::subtitle::OpaqueTrack::from_compiled(
            std::iter::once(&indirect),
            &track.styles,
            &opaque_track.script_info(),
        );

        'inner: for now_offset in (0..sline.duration.0).step_by(100) {
            let now = sline.start.0 + now_offset;

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
                    now_offset, sline.start.0
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

fn parse_round_trip(sline: &subtitle::Sline) -> subtitle::ass::Event {
    let (global, spans) = nde::tags::parse(&sline.text);
    let emitted = nde::tags::emit(&global, &spans);

    subtitle::ass::Event {
        start: sline.start,
        duration: sline.duration,
        layer_index: sline.layer_index,
        style_index: sline.style_index,
        margins: sline.margins,
        text: Cow::Owned(emitted),
        read_order: 0,
        name: Cow::Borrowed(""),
        effect: Cow::Borrowed(""),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct AssImage {
    width: i32,
    height: i32,
    dest_x: i32,
    dest_y: i32,
    stride: i32,
    colour: subtitle::Colour,
    data_hash: u64,
}

impl AssImage {
    pub fn from<BH: BuildHasher>(image: &media::subtitle::Image, build_hasher: &BH) -> Self {
        Self {
            width: image.metadata.w,
            height: image.metadata.h,
            dest_x: image.metadata.dst_x,
            dest_y: image.metadata.dst_y,
            stride: image.metadata.stride,
            colour: subtitle::Colour::unpack(image.metadata.color),
            data_hash: build_hasher.hash_one(image.bitmap),
        }
    }
}
