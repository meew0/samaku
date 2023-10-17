pub use ass::Image;

use crate::nde::tags::Colour;
use crate::{model, subtitle, view};

use super::bindings::{ass, c_string};

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
        let track = ass::LIBRARY.read_memory(data.as_bytes(), None).unwrap();

        OpaqueTrack { internal: track }
    }

    /// Convert data from our representation into libass'.
    ///
    /// # Panics
    /// Panics if libass fails to construct a new subtitle track.
    pub fn from_compiled<'a>(
        events: impl IntoIterator<Item = &'a subtitle::CompiledEvent<'a>>,
        styles: &[subtitle::Style],
        metadata: &subtitle::ScriptInfo,
    ) -> OpaqueTrack {
        let mut track = ass::LIBRARY
            .new_track()
            .expect("failed to construct new track");

        track.set_header(metadata);

        for event in events {
            track.alloc_event();
            *track.events_mut().last_mut().unwrap() = ass::event_to_raw(event);
        }

        for style in styles {
            track.alloc_style();
            *track.styles_mut().last_mut().unwrap() = ass::style_to_raw(style);
        }

        OpaqueTrack { internal: track }
    }

    #[must_use]
    pub fn to_sline_track(&self) -> subtitle::SlineTrack {
        subtitle::SlineTrack {
            slines: self.slines(),
            styles: self.styles(),
            extradata: subtitle::Extradata::default(),
        }
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

    fn slines(&self) -> Vec<subtitle::Sline> {
        self.internal
            .events()
            .iter()
            .map(ass::raw_event_to_sline)
            .collect::<Vec<_>>()
    }

    fn styles(&self) -> Vec<subtitle::Style> {
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
        let mut renderer = ass::LIBRARY.renderer_init().unwrap();
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
    renderer.set_fonts(
        Some(c_string(
            "/usr/share/fonts/alegreya-sans/AlegreyaSans-Regular.ttf",
        )),
        &c_string("Alegreya Sans"),
        ass::FontProvider::Autodetect,
        None,
        false,
    );
}

/// Convert an image from libass' representation into iced's.
///
/// # Panics
/// Panics if the libass image has invalid metadata (e.g. negative dimensions).
#[must_use]
pub fn ass_image_to_iced(
    ass_image: &ass::Image,
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
