use crate::{subtitle, view};

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
    pub fn parse(data: String) -> OpaqueTrack {
        let track = ass::LIBRARY.read_memory(data.as_bytes(), None).unwrap();

        OpaqueTrack { internal: track }
    }

    /// Convert data from our representation into libass'.
    pub fn from_compiled<'a>(
        events: impl IntoIterator<Item = &'a subtitle::ass::Event<'a>>,
        metadata: &subtitle::SlineTrack,
    ) -> OpaqueTrack {
        let mut track = ass::LIBRARY
            .new_track()
            .expect("failed to construct new track");

        track.set_header(&subtitle::ass::TrackHeader {
            play_res: metadata.playback_resolution,
            timer: 0.0,
            wrap_style: subtitle::WrapStyle::SmartEven,
            scaled_border_and_shadow: true,
            kerning: false,
            language: None,
            ycbcr_matrix: subtitle::ass::YCbCrMatrix::None,
            name: None,
        });

        for event in events.into_iter() {
            track.alloc_event();
            *track.events_mut().last_mut().unwrap() = ass::event_to_raw(event);
        }

        for style in metadata.styles.iter() {
            track.alloc_style();
            *track.styles_mut().last_mut().unwrap() = ass::style_to_raw(style);
        }

        OpaqueTrack { internal: track }
    }

    pub fn to_sline_track(&self) -> subtitle::SlineTrack {
        let header = self.internal.header();

        subtitle::SlineTrack {
            slines: self.slines(),
            styles: self.styles(),
            playback_resolution: header.play_res,
        }
    }

    pub fn num_events(&self) -> usize {
        self.internal.events().len()
    }

    pub fn num_styles(&self) -> usize {
        self.internal.styles().len()
    }

    fn slines(&self) -> Vec<subtitle::Sline> {
        self.internal
            .events()
            .iter()
            .map(|raw_event| ass::raw_event_to_sline(raw_event))
            .collect::<Vec<_>>()
    }

    fn styles(&self) -> Vec<subtitle::Style> {
        self.internal
            .styles()
            .iter()
            .map(|raw_style| ass::style_from_raw(raw_style))
            .collect::<Vec<_>>()
    }
}

#[derive(Debug)]
pub struct Renderer {
    internal: ass::Renderer,
}

impl Renderer {
    pub fn new() -> Renderer {
        let mut renderer = ass::LIBRARY.renderer_init().unwrap();
        renderer_set_fonts_default(&mut renderer);
        Renderer { internal: renderer }
    }

    pub fn render_subtitles_onto_base(
        &mut self,
        subtitles: OpaqueTrack,
        base: iced::widget::image::Handle,
        frame: i32,
        frame_rate: super::video::FrameRate,
        frame_size: subtitle::Resolution,
        storage_size: subtitle::Resolution,
    ) -> Vec<view::widget::StackedImage<iced::widget::image::Handle>> {
        let now: i64 = ass::frame_to_ms(frame, frame_rate.into());

        let mut result: Vec<view::widget::StackedImage<iced::widget::image::Handle>> = vec![];
        result.push(view::widget::StackedImage {
            handle: base,
            x: 0,
            y: 0,
        });

        self.internal.set_frame_size(frame_size.x, frame_size.y);
        self.internal
            .set_storage_size(storage_size.x, storage_size.y);

        self.internal
            .render_frame(&subtitles.internal, now, &mut |image| {
                result.push(ass_image_to_iced(image))
            });

        result
    }
}

pub fn renderer_set_fonts_default(renderer: &mut ass::Renderer) {
    renderer.set_fonts(
        Some(c_string(
            "/usr/share/fonts/alegreya-sans/AlegreyaSans-Regular.ttf",
        )),
        c_string("Alegreya Sans"),
        ass::FontProvider::Autodetect,
        None,
        false,
    );
}

pub fn ass_image_to_iced(
    ass_image: &ass::Image,
) -> view::widget::StackedImage<iced::widget::image::Handle> {
    let width = ass_image.metadata.w as usize;
    let height = ass_image.metadata.h as usize;
    let pitch = width * 4;
    let out_len = pitch * height;

    // Potential optimisation: allocate as 32-bit, transmute to 8-bit later
    let mut out = vec![0; out_len];

    let subtitle::Colour {
        red,
        green,
        blue,
        transparency,
    } = subtitle::Colour::unpack(ass_image.metadata.color);
    let alpha: u16 = 255 - transparency as u16;

    for row in 0..height {
        let row_read_start = row * ass_image.metadata.stride as usize;
        let row_read_ptr = &ass_image.bitmap[row_read_start..(row_read_start + width)];

        let row_write_start = row * pitch;
        let row_write_ptr = &mut out[row_write_start..(row_write_start + width * 4)];

        for col in 0..width {
            row_write_ptr[col * 4] = red;
            row_write_ptr[col * 4 + 1] = green;
            row_write_ptr[col * 4 + 2] = blue;
            row_write_ptr[col * 4 + 3] = ((alpha * row_read_ptr[col] as u16) >> 8) as u8;
        }
    }

    let handle = iced::widget::image::Handle::from_pixels(width as u32, height as u32, out);
    view::widget::StackedImage {
        handle,
        x: ass_image.metadata.dst_x,
        y: ass_image.metadata.dst_y,
    }
}
