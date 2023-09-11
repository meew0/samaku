use std::ffi::OsStr;
use std::path::Path;

use super::bindings::vapoursynth;
pub use vapoursynth::FrameRate;

const DEFAULT_SCRIPT: &str = include_str!("default_scripts/video.py");

const KF_KEY: &str = "__aegi_keyframes";
const TC_KEY: &str = "__aegi_timecodes";
const AUDIO_KEY: &str = "__aegi_hasaudio";

use super::bindings::c_string;

pub struct Video {
    script: vapoursynth::Script,
    node: vapoursynth::Node,
    pub current_frame: i32,
    pub frame_rate: FrameRate,
    pub width: i32,
    pub height: i32,
}

impl Video {
    pub fn load<P: AsRef<Path>>(filename: P) -> Video {
        let script = vapoursynth::open_script(DEFAULT_SCRIPT, filename);

        let node: vapoursynth::Node = script.get_output_node(0).unwrap();
        println!("Output node is video: {}", node.is_video());

        let vi = node.get_video_info().unwrap();
        if !vi.is_constant_video_format() {
            panic!("Non-constant video format");
        }

        let width = vi.get_width();
        let height = vi.get_height();

        let frame_rate = vi.get_frame_rate();
        println!("Frame rate: {:?}", frame_rate);

        let mut clipinfo_owned = vapoursynth::OwnedMap::create_map().unwrap();
        let clipinfo = clipinfo_owned.as_mut();
        script.get_variable(c_string(KF_KEY), clipinfo);
        script.get_variable(c_string(TC_KEY), clipinfo);
        script.get_variable(c_string(AUDIO_KEY), clipinfo);

        let num_kf = clipinfo.as_const().num_elements(c_string(KF_KEY));
        let num_tc = clipinfo.as_const().num_elements(c_string(TC_KEY));
        let has_audio = match clipinfo.as_const().get_int(c_string(AUDIO_KEY), 0) {
            Ok(val) => val != 0,
            Err(_) => false,
        };

        // TODO: keyframes and timecodes

        let frame = node.get_frame(0).unwrap();
        let props = frame.get_properties_ro().unwrap();

        let dar = match props.get_int(c_string("_SARNum"), 0) {
            Ok(sarn) => match props.get_int(c_string("_SARDen"), 0) {
                Ok(sard) => {
                    (vi.get_width() as i64 * sarn) as f64 / (vi.get_height() as i64 * sard) as f64
                }
                Err(_) => 0.0,
            },
            Err(_) => 0.0,
        };

        println!("dar = {}", dar);

        let color_space = vapoursynth::color_matrix_description(&vi, &props);
        println!("Color space: {}", color_space);

        let out_node = if vi.is_rgb24() {
            node
        } else {
            let mut resize = script.get_core().get_resize_plugin().unwrap();

            let mut args_owned = vapoursynth::OwnedMap::create_map().unwrap();
            let mut args = args_owned.as_mut();
            vapoursynth::init_resize(&vi, &mut args, &props);
            args.append_node(c_string("clip"), node);

            let mut result_owned = resize.invoke(c_string("Bicubic"), args.as_const());
            let result = result_owned.as_mut();

            match result.as_const().get_error() {
                Some(err) => panic!("Failed to convert to RGB24: {}", err),
                None => (),
            }

            let new_node = result.as_const().get_node(c_string("clip"), 0).unwrap();

            let new_frame = new_node.get_frame(0).unwrap();
            let new_color_space = vapoursynth::color_matrix_description(
                &new_node.get_video_info().unwrap(),
                &new_frame.get_properties_ro().unwrap(),
            );
            println!("New color space: {}", new_color_space);
            new_node
        };

        Video {
            script,
            node: out_node,
            current_frame: 0,
            frame_rate,
            width,
            height,
        }
    }

    pub fn get_frame(&self, n: i32) -> iced::widget::image::Handle {
        let instant = std::time::Instant::now();

        let vs_frame = self.node.get_frame(n).unwrap();

        println!("Obtaining frame took {:.2?}", instant.elapsed());

        let video_format = vs_frame.get_video_format().unwrap();
        if !video_format.is_rgb24() {
            panic!("Frame is not in RGB24 format");
        }

        let width = vs_frame.get_width(0);
        let height = vs_frame.get_height(0);
        let pitch = width as usize * 4;
        let out_len = pitch * height as usize;
        let mut out = vec![0; out_len];

        let instant2 = std::time::Instant::now();

        // RGB
        for plane in 0..3 {
            let stride = vs_frame.get_stride(plane) as usize;
            let read_ptr = vs_frame.get_read_ptr(plane);
            let write_ptr = &mut out[(plane as usize)..];
            let rows = vs_frame.get_height(plane) as usize;
            let cols = vs_frame.get_width(plane) as usize;

            for row in 0..rows {
                let row_start_read = stride * row;
                let row_read_ptr = &read_ptr[row_start_read..(row_start_read + cols)];

                let row_start_write = pitch * row;
                let row_write_ptr =
                    &mut write_ptr[row_start_write..(row_start_write + 4 * cols - 3)];

                for col in 0..cols {
                    row_write_ptr[4 * col] = row_read_ptr[col];
                }
            }
        }

        println!("Copying frame took {:.2?}", instant2.elapsed());

        // Alpha
        let write_ptr = &mut out[3..];
        write_ptr.chunks_mut(4).for_each(|chunk| chunk[0] = 0xff);

        iced::widget::image::Handle::from_pixels(width as u32, height as u32, out)
    }

    pub fn get_current_frame(&self) -> iced::widget::image::Handle {
        self.get_frame(self.current_frame)
    }

    pub fn next_frame(&mut self) {
        self.current_frame += 1;
    }

    pub fn previous_frame(&mut self) {
        self.current_frame -= 1;
    }
}
