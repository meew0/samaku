pub use ass::Image;

use crate::nde::tags::Colour;
use crate::{model, resources, subtitle, view};

use super::bindings::ass;

/// The global libass instance.
static LIBRARY: once_cell::sync::Lazy<ass::Library> = once_cell::sync::Lazy::new(library_init);

fn library_init() -> ass::Library {
    let library = ass::Library::init().expect("ASS library initialisation failed");

    // Load Barlow, our UI font we package anyway, into libass so it can be used as a fallback font
    // in case no system fonts are available for whatever reason
    library.add_font("Barlow", resources::BARLOW);

    library
}

/// Set the global libass message callback. The provided closure will be called on every log message
/// produced by libass.
pub fn set_libass_callback<F: FnMut(i32, String) + 'static>(callback: F) {
    LIBRARY.set_message_callback(callback);
}

/// Set the global libass message callback to one that prints all messages level 5 and below to
/// stdout, to avoid cluttering the console output in tests.
pub fn set_libass_test_callback() {
    set_libass_callback(|level, string| {
        if level <= 5 {
            println!("[ass] [level {level}] {string}");
        }
    });
}

pub struct OpaqueTrack {
    internal: ass::Track,
}

/// Represents an opaque ASS subtitle track.
/// Can be converted as a whole to and from some other formats,
/// but does not provide a way to inspect or modify its interior.
impl OpaqueTrack {
    /// Parse subtitles represented in the text-based ASS format.
    /// Beyond the individual events, the string must also contain
    /// all the metadata libass needs to correctly parse them.
    ///
    /// # Panics
    /// Panics if libass fails to parse the data.
    pub fn parse(data: &String) -> OpaqueTrack {
        let track = LIBRARY.read_memory(data.as_bytes(), None).unwrap();
        OpaqueTrack { internal: track }
    }

    /// Convert data from our representation into libass'.
    ///
    /// # Panics
    /// Panics if libass fails to construct a new subtitle track or when there are more events than
    /// would fit into an `i32`.
    pub fn from_compiled<'a>(
        events: impl IntoIterator<Item = &'a subtitle::Event<'a>>,
        styles: &[subtitle::Style],
        metadata: &subtitle::ScriptInfo,
    ) -> OpaqueTrack {
        let mut track = LIBRARY.new_track().expect("failed to construct new track");

        track.set_header(metadata);

        assert_eq!(track.events().len(), 0); // No events should exist yet
        for (read_index, event) in events.into_iter().enumerate() {
            track.alloc_event();
            *track.events_mut().last_mut().unwrap() =
                ass::event_to_raw(event, i32::try_from(read_index).unwrap());
        }

        track.resize_styles(styles.len());
        assert_eq!(track.styles().len(), styles.len());
        for (raw_style, style) in track.styles_mut().iter_mut().zip(styles) {
            *raw_style = ass::style_to_raw(style);
        }

        OpaqueTrack { internal: track }
    }

    #[must_use]
    pub fn to_event_track(&self) -> subtitle::EventTrack {
        subtitle::EventTrack::from_vec(self.events())
    }

    #[must_use]
    pub fn script_info(&self) -> subtitle::ScriptInfo {
        self.internal.header()
    }

    #[must_use]
    pub fn num_events(&self) -> usize {
        self.internal.events().len()
    }

    #[must_use]
    pub fn num_styles(&self) -> usize {
        self.internal.styles().len()
    }

    fn events(&self) -> Vec<subtitle::Event<'static>> {
        self.internal
            .events()
            .iter()
            .map(ass::event_from_raw)
            .collect::<Vec<_>>()
    }

    pub fn styles(&self) -> Vec<subtitle::Style> {
        self.internal
            .styles()
            .iter()
            .map(ass::style_from_raw)
            .collect::<Vec<_>>()
    }
}

#[derive(Debug)]
pub struct Renderer {
    internal: ass::Renderer,
}

impl Renderer {
    /// Create a new renderer by calling into libass.
    ///
    /// # Panics
    /// Panics if libass fails to create a new renderer.
    pub fn new() -> Renderer {
        let mut renderer = LIBRARY.renderer_init().unwrap();
        renderer_set_fonts_default(&mut renderer);
        Renderer { internal: renderer }
    }

    pub fn render_subtitles_onto_base(
        &mut self,
        subtitles: &OpaqueTrack,
        base: iced::widget::image::Handle,
        frame: model::FrameNumber,
        frame_rate: super::video::FrameRate,
        frame_size: subtitle::Resolution,
        storage_size: subtitle::Resolution,
    ) -> Vec<view::widget::StackedImage<iced::widget::image::Handle>> {
        let now: i64 = frame_rate.frame_to_ms(frame);

        let mut result: Vec<view::widget::StackedImage<iced::widget::image::Handle>> = vec![];
        result.push(view::widget::StackedImage {
            handle: base,
            x: 0,
            y: 0,
        });

        self.render_subtitles_with_callback(
            subtitles,
            now,
            frame_size,
            storage_size,
            &mut |image| result.push(ass_image_to_iced(image)),
        );

        result
    }

    pub fn render_subtitles_with_callback<F: FnMut(&Image)>(
        &mut self,
        subtitles: &OpaqueTrack,
        now: i64,
        frame_size: subtitle::Resolution,
        storage_size: subtitle::Resolution,
        callback: &mut F,
    ) {
        self.internal.set_frame_size(frame_size.x, frame_size.y);
        self.internal
            .set_storage_size(storage_size.x, storage_size.y);

        self.internal
            .render_frame(&subtitles.internal, now, callback);
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn renderer_set_fonts_default(renderer: &mut ass::Renderer) {
    renderer.set_fonts(None, "Barlow", ass::FontProvider::Autodetect, None, false);
}

/// Convert an image from libass' representation into iced's.
///
/// # Panics
/// Panics if the libass image has invalid metadata (e.g. negative dimensions).
#[must_use]
pub fn ass_image_to_iced(
    ass_image: &Image,
) -> view::widget::StackedImage<iced::widget::image::Handle> {
    let width: usize = ass_image
        .metadata
        .w
        .try_into()
        .expect("image width should not be negative");
    let height: usize = ass_image
        .metadata
        .h
        .try_into()
        .expect("image height should not be negative");
    let pitch = width * 4;
    let out_len = pitch * height;

    // Potential optimisation: allocate as 32-bit, transmute to 8-bit later
    let mut out = vec![0; out_len];

    let (Colour { red, green, blue }, transparency) =
        subtitle::unpack_colour_and_transparency_rgbt(ass_image.metadata.color);
    let alpha: u16 = 255 - u16::from(transparency.rendered());

    let stride: usize = ass_image
        .metadata
        .stride
        .try_into()
        .expect("stride should not be negative");

    for row in 0..height {
        let row_read_start = row * stride;
        let row_read_ptr = &ass_image.bitmap[row_read_start..(row_read_start + width)];

        let row_write_start = row * pitch;
        let row_write_ptr = &mut out[row_write_start..(row_write_start + width * 4)];

        for col in 0..width {
            row_write_ptr[col * 4] = red;
            row_write_ptr[col * 4 + 1] = green;
            row_write_ptr[col * 4 + 2] = blue;
            row_write_ptr[col * 4 + 3] = ((alpha * u16::from(row_read_ptr[col])) >> 8) as u8;
        }
    }

    let handle = iced::widget::image::Handle::from_pixels(
        u32::try_from(width).expect("image width should fit into a `u32`"),
        u32::try_from(height).expect("image height should fit into a `u32`"),
        out,
    );
    view::widget::StackedImage {
        handle,
        x: ass_image.metadata.dst_x,
        y: ass_image.metadata.dst_y,
    }
}

#[cfg(test)]
mod tests {
    use crate::media;
    use crate::nde::tags::Transparency;

    use super::*;

    /// Test to verify that our handling of events and their styles is lossless.
    #[test]
    fn style_colours() {
        const ASS_FILE: &str = include_str!("../../test_files/style_colours.ass");
        const FRAME_SIZE: subtitle::Resolution = subtitle::Resolution { x: 192, y: 108 };
        const FRAME_RATE: media::FrameRate = media::FrameRate {
            numerator: 24,
            denominator: 1,
        };

        // Expected colours
        const WHITE: Colour = Colour {
            red: 255,
            green: 255,
            blue: 255,
        };
        const BLACK: Colour = Colour {
            red: 0,
            green: 0,
            blue: 0,
        };
        const OPAQUE: Transparency = Transparency(0);
        const PRIMARY_2_COLOUR: Colour = Colour {
            red: 53,
            green: 162,
            blue: 228,
        };
        const PRIMARY_2_TRANSPARENCY: Transparency = Transparency(18);
        const BORDER_2_COLOUR: Colour = Colour {
            red: 179,
            green: 230,
            blue: 68,
        };
        const BORDER_2_TRANSPARENCY: Transparency = Transparency(66);
        const SHADOW_2_COLOUR: Colour = Colour {
            red: 189,
            green: 25,
            blue: 113,
        };
        const SHADOW_2_TRANSPARENCY: Transparency = Transparency(136);

        set_libass_test_callback();

        let opaque_track = OpaqueTrack::parse(&ASS_FILE.to_owned());

        // There will be one extra for libass' default style
        assert_eq!(opaque_track.styles().len(), 3);
        let default = opaque_track.internal.events()[0].Style;
        let alternate = opaque_track.internal.events()[1].Style;
        println!("{default} {alternate}");

        #[allow(clippy::cast_sign_loss)]
        let default_usize = default as usize;
        #[allow(clippy::cast_sign_loss)]
        let alternate_usize = alternate as usize;

        // Verify that colours are as we expect
        assert_eq!(opaque_track.styles()[default_usize].primary_colour, WHITE);
        assert_eq!(
            opaque_track.styles()[default_usize].primary_transparency,
            OPAQUE
        );
        assert_eq!(opaque_track.styles()[default_usize].border_colour, BLACK);
        assert_eq!(
            opaque_track.styles()[default_usize].border_transparency,
            OPAQUE
        );
        assert_eq!(opaque_track.styles()[default_usize].shadow_colour, BLACK);
        assert_eq!(
            opaque_track.styles()[default_usize].shadow_transparency,
            OPAQUE
        );
        assert_eq!(
            opaque_track.styles()[alternate_usize].primary_colour,
            PRIMARY_2_COLOUR
        );
        assert_eq!(
            opaque_track.styles()[alternate_usize].primary_transparency,
            PRIMARY_2_TRANSPARENCY
        );
        assert_eq!(
            opaque_track.styles()[alternate_usize].border_colour,
            BORDER_2_COLOUR
        );
        assert_eq!(
            opaque_track.styles()[alternate_usize].border_transparency,
            BORDER_2_TRANSPARENCY
        );
        assert_eq!(
            opaque_track.styles()[alternate_usize].shadow_colour,
            SHADOW_2_COLOUR
        );
        assert_eq!(
            opaque_track.styles()[alternate_usize].shadow_transparency,
            SHADOW_2_TRANSPARENCY
        );

        // Render the opaque track directly with libass
        let mut renderer = Renderer::new();
        let mut colours: Vec<u32> = vec![];
        renderer.render_subtitles_with_callback(
            &opaque_track,
            1000,
            FRAME_SIZE,
            FRAME_SIZE,
            &mut |image| colours.push(image.metadata.color),
        );

        // We cannot assume libass to consistently order the images, so sort the vec
        colours.sort_unstable();
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[0]),
            (BLACK, OPAQUE)
        );
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[1]),
            (BLACK, OPAQUE)
        );
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[2]),
            (WHITE, OPAQUE)
        );

        // And again for the other line
        colours.clear();
        renderer.render_subtitles_with_callback(
            &opaque_track,
            3000,
            FRAME_SIZE,
            FRAME_SIZE,
            &mut |image| colours.push(image.metadata.color),
        );
        colours.sort_unstable();
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[0]),
            (PRIMARY_2_COLOUR, PRIMARY_2_TRANSPARENCY)
        );
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[1]),
            (BORDER_2_COLOUR, BORDER_2_TRANSPARENCY)
        );
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[2]),
            (SHADOW_2_COLOUR, SHADOW_2_TRANSPARENCY)
        );

        // Do the whole thing again, going through a round trip of ass -> stored event -> ass
        let event_track = opaque_track.to_event_track();
        let styles = opaque_track.styles();
        assert_eq!(
            event_track[subtitle::EventIndex(0)].style_index,
            default_usize
        );
        assert_eq!(
            event_track[subtitle::EventIndex(1)].style_index,
            alternate_usize
        );
        assert_eq!(styles[default_usize].primary_colour, WHITE);
        assert_eq!(styles[default_usize].primary_transparency, OPAQUE);
        assert_eq!(styles[default_usize].border_colour, BLACK);
        assert_eq!(styles[default_usize].border_transparency, OPAQUE);
        assert_eq!(styles[default_usize].shadow_colour, BLACK);
        assert_eq!(styles[default_usize].shadow_transparency, OPAQUE);
        assert_eq!(styles[alternate_usize].primary_colour, PRIMARY_2_COLOUR);
        assert_eq!(
            styles[alternate_usize].primary_transparency,
            PRIMARY_2_TRANSPARENCY
        );
        assert_eq!(styles[alternate_usize].border_colour, BORDER_2_COLOUR);
        assert_eq!(
            styles[alternate_usize].border_transparency,
            BORDER_2_TRANSPARENCY
        );
        assert_eq!(styles[alternate_usize].shadow_colour, SHADOW_2_COLOUR);
        assert_eq!(
            styles[alternate_usize].shadow_transparency,
            SHADOW_2_TRANSPARENCY
        );

        let context = subtitle::compile::Context {
            frame_rate: FRAME_RATE,
        };
        let compiled_events =
            event_track.compile(&subtitle::Extradata::default(), &context, 24, Some(1));
        assert_eq!(compiled_events[0].style_index, default_usize);
        assert_eq!(compiled_events[1].style_index, alternate_usize);

        let script_info = subtitle::ScriptInfo {
            playback_resolution: FRAME_SIZE,
            ..Default::default()
        };

        let opaque2 = OpaqueTrack::from_compiled(&compiled_events, &styles, &script_info);
        renderer = Renderer::new();
        colours.clear();
        renderer.render_subtitles_with_callback(
            &opaque2,
            1000,
            FRAME_SIZE,
            FRAME_SIZE,
            &mut |image| colours.push(image.metadata.color),
        );
        colours.sort_unstable();
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[0]),
            (BLACK, OPAQUE)
        );
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[1]),
            (BLACK, OPAQUE)
        );
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[2]),
            (WHITE, OPAQUE)
        );
        colours.clear();
        renderer.render_subtitles_with_callback(
            &opaque2,
            3000,
            FRAME_SIZE,
            FRAME_SIZE,
            &mut |image| colours.push(image.metadata.color),
        );
        colours.sort_unstable();
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[0]),
            (PRIMARY_2_COLOUR, PRIMARY_2_TRANSPARENCY)
        );
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[1]),
            (BORDER_2_COLOUR, BORDER_2_TRANSPARENCY)
        );
        assert_eq!(
            subtitle::unpack_colour_and_transparency_rgbt(colours[2]),
            (SHADOW_2_COLOUR, SHADOW_2_TRANSPARENCY)
        );
    }
}
