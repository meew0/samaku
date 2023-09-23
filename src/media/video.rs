use std::path::Path;

pub use vapoursynth::FrameRate;

use super::bindings::{c_string, vapoursynth};

const DEFAULT_SCRIPT: &str = include_str!("default_scripts/video.py");

const KF_KEY: &str = "__aegi_keyframes";
const TC_KEY: &str = "__aegi_timecodes";
const AUDIO_KEY: &str = "__aegi_hasaudio";

#[derive(Debug, Clone, Copy)]
pub struct VideoMetadata {
    pub frame_rate: FrameRate,
    pub width: i32,
    pub height: i32,
}

pub struct Video {
    _script: vapoursynth::Script,
    node: vapoursynth::Node,
    pub metadata: VideoMetadata,
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
        println!(
            "num_kf: {}, num_tc: {}, has_audio: {}",
            num_kf, num_tc, has_audio,
        );

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
            let args = args_owned.as_mut();
            vapoursynth::init_resize(&vi, args, &props);
            args.append_node(c_string("clip"), node);

            let mut result_owned = resize.invoke(c_string("Bicubic"), args.as_const());
            let result = result_owned.as_mut();

            if let Some(err) = result.as_const().get_error() {
                panic!("Failed to convert to RGB24: {}", err);
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
            _script: script,
            node: out_node,
            metadata: VideoMetadata {
                frame_rate,
                width,
                height,
            },
        }
    }

    fn get_frame_internal(&self, n: i32) -> vapoursynth::Frame {
        let vs_frame = self.node.get_frame(n).unwrap();

        let video_format = vs_frame.get_video_format().unwrap();
        if !video_format.is_rgb24() {
            panic!("Frame is not in RGB24 format");
        }

        vs_frame
    }

    pub fn get_iced_frame(&self, n: i32) -> iced::widget::image::Handle {
        let instant = std::time::Instant::now();
        let vs_frame = self.get_frame_internal(n);
        let elapsed_obtain = instant.elapsed();

        let width = vs_frame.get_width(0);
        let height = vs_frame.get_height(0);

        let pitch = width as usize * 4;
        let out_len = pitch * height as usize;
        let mut out = vec![0; out_len];

        let instant2 = std::time::Instant::now();

        // Use libp2p, which we are linking to anyway because of BestSource,
        // for high performance (SIMD) packing
        let p2p_params = bestsource_sys::p2p_buffer_param {
            src: [
                vs_frame.get_read_ptr(0).as_ptr() as *const std::ffi::c_void,
                vs_frame.get_read_ptr(1).as_ptr() as *const std::ffi::c_void,
                vs_frame.get_read_ptr(2).as_ptr() as *const std::ffi::c_void,
                std::ptr::null(),
            ],
            dst: [
                out.as_mut_ptr() as *mut std::ffi::c_void,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ],
            src_stride: [
                vs_frame.get_stride(0),
                vs_frame.get_stride(1),
                vs_frame.get_stride(2),
                0,
            ],
            dst_stride: [pitch as isize, 0, 0, 0],
            width: width as u32,
            height: height as u32,
            packing: bestsource_sys::p2p_packing_p2p_rgba32_be,
        };

        unsafe {
            bestsource_sys::p2p_pack_frame(&p2p_params, bestsource_sys::P2P_ALPHA_SET_ONE as u64);
        }

        let elapsed_copy = instant2.elapsed();
        println!(
            "Frame profiling [display]: obtaining frame {} took {:.2?}, packing it took {:.2?}",
            n, elapsed_obtain, elapsed_copy
        );

        iced::widget::image::Handle::from_pixels(width as u32, height as u32, out)
    }

    pub fn get_libmv_patch(
        &self,
        n: i32,
        request: super::motion::PatchRequest,
    ) -> super::motion::PatchResponse {
        let instant = std::time::Instant::now();
        let vs_frame = self.get_frame_internal(n);
        let elapsed_obtain = instant.elapsed();
        let frame_width: usize = vs_frame.get_width(0).try_into().unwrap();
        let frame_height: usize = vs_frame.get_height(0).try_into().unwrap();

        // Fit request parameters into the frame bounds
        let true_left = request.left.max(0.0).floor() as usize;
        let true_top = request.top.max(0.0).floor() as usize;
        let true_width = request.width.min((frame_width - true_left) as f64).ceil() as usize;
        let true_height = request.height.min((frame_height - true_top) as f64).ceil() as usize;

        let mut out = vec![0.0_f32; true_width * true_height];

        let instant2 = std::time::Instant::now();

        // The conversion coefficients used by Blender, divided by 255
        const GREYSCALE_COEFFICIENTS: [f32; 3] = [0.000833373, 0.00280471, 0.00028314];

        // Assumes all frames are the same size. They should be.
        for plane in 0..3 {
            let stride = vs_frame.get_stride(plane) as usize;
            let read_ptr = vs_frame.get_read_ptr(plane);
            let coefficient = GREYSCALE_COEFFICIENTS[plane as usize];

            for row in 0..true_height {
                let row_start_read = stride * (true_top + row) + true_left;
                let row_read_ptr = &read_ptr[row_start_read..(row_start_read + true_width)];

                let row_start_write = true_width * row;
                let row_write_ptr = &mut out[row_start_write..(row_start_write + true_width)];

                for col in 0..true_width {
                    row_write_ptr[col] += coefficient * row_read_ptr[col] as f32;
                }
            }
        }

        let elapsed_copy = instant2.elapsed();
        println!(
            "Frame profiling [motion tracking]: obtaining frame {} took {:.2?}, converting it took {:.2?}",
            n, elapsed_obtain, elapsed_copy
        );

        super::motion::PatchResponse {
            data: out,
            left: true_left,
            top: true_top,
            width: true_width,
            height: true_height,
        }
    }
}
