use crate::view;

use super::bindings::{ass, c_string};

pub struct Subtitles {
    renderer: ass::Renderer,
    track: ass::Track,
}

pub fn init_renderer(width: i32, height: i32) -> ass::Renderer {
    let mut renderer = ass::LIBRARY.renderer_init().unwrap();
    renderer.set_frame_size(width, height);
    renderer.set_storage_size(width, height);
    renderer.set_fonts(
        Some(c_string(
            "/usr/share/fonts/alegreya-sans/AlegreyaSans-Regular.ttf",
        )),
        c_string("Alegreya Sans"),
        ass::FontProvider::Autodetect,
        None,
        false,
    );
    renderer
}

impl Subtitles {
    pub fn load_utf8(data: String, width: i32, height: i32) -> Subtitles {
        let track = ass::LIBRARY.read_memory(data.as_bytes(), None).unwrap();

        Subtitles {
            track,
            renderer: init_renderer(width, height),
        }
    }

    pub fn render_onto(
        &self,
        base: iced::widget::image::Handle,
        frame: i32,
        frame_rate: super::video::FrameRate,
    ) -> Vec<view::widget::StackedImage<iced::widget::image::Handle>> {
        let now: i64 = ass::frame_to_ms(frame, frame_rate.into());

        let mut result: Vec<view::widget::StackedImage<iced::widget::image::Handle>> = vec![];
        result.push(view::widget::StackedImage {
            handle: base,
            x: 0,
            y: 0,
        });

        self.renderer.render_frame(&self.track, now, &mut |image| {
            result.push(ass_image_to_iced(image))
        });

        println!("Rendered {} subtitle images", result.len() - 1);

        result
    }
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

    let color: u32 = ass_image.metadata.color;
    let r: u8 = ((color & 0xff000000) >> 24).try_into().unwrap();
    let g: u8 = ((color & 0x00ff0000) >> 16).try_into().unwrap();
    let b: u8 = ((color & 0x0000ff00) >> 8).try_into().unwrap();
    let transparency: u8 = ((color & 0x000000ff) >> 0).try_into().unwrap();
    let a: u16 = 255 - transparency as u16;

    for row in 0..height {
        let row_read_start = row * ass_image.metadata.stride as usize;
        let row_read_ptr = &ass_image.bitmap[row_read_start..(row_read_start + width)];

        let row_write_start = row * pitch;
        let row_write_ptr = &mut out[row_write_start..(row_write_start + width * 4)];

        for col in 0..width {
            row_write_ptr[col * 4] = r;
            row_write_ptr[col * 4 + 1] = g;
            row_write_ptr[col * 4 + 2] = b;
            row_write_ptr[col * 4 + 3] = ((a * row_read_ptr[col] as u16) >> 8) as u8;
        }
    }

    let handle = iced::widget::image::Handle::from_pixels(width as u32, height as u32, out);
    view::widget::StackedImage {
        handle,
        x: ass_image.metadata.dst_x,
        y: ass_image.metadata.dst_y,
    }
}
