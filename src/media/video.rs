use std::path::Path;

use thiserror::Error;

pub use vapoursynth::FrameRate;

use crate::model;

use super::bindings::{c_string, vapoursynth};

const DEFAULT_SCRIPT: &str = include_str!("default_scripts/video.py");

const KF_KEY: &str = "__aegi_keyframes";
const TC_KEY: &str = "__aegi_timecodes";
const AUDIO_KEY: &str = "__aegi_hasaudio";

#[derive(Debug, Clone, Copy)]
pub struct Metadata {
    pub frame_rate: FrameRate,
    pub width: i32,
    pub height: i32,
}

pub struct Video {
    _script: vapoursynth::Script,
    node: vapoursynth::Node,
    pub metadata: Metadata,
}

impl Video {
    /// Load the video from the given file using Vapoursynth and LSMASHSource.
    ///
    /// # Errors
    /// Returns an error in the following scenarios:
    ///  1. Failed to open the file
    ///  2. The file is not a video
    ///  3. The video format is non-constant
    ///  4. Vapoursynth fails to create necessary data structures
    ///  5. The video does not contain any frames, or the frame data cannot be read
    ///  6. The Vapoursynth resize plugin is unavailable
    ///  7. The colour space conversion to RGB24 fails
    pub fn load<P: AsRef<Path>>(filename: P) -> Result<Video, LoadError> {
        let Some(script) = vapoursynth::open_script(DEFAULT_SCRIPT, filename) else {
            return Err(LoadError::FailedToOpen);
        };

        let Some(node) = script.get_output_node(0) else {
            return Err(LoadError::FailedToGetNode);
        };
        println!("Output node is video: {}", node.is_video());

        let Some(vi) = node.get_video_info() else {
            return Err(LoadError::NotAVideo);
        };
        if !vi.is_constant_video_format() {
            return Err(LoadError::NonConstantFormat);
        }

        let width = vi.get_width();
        let height = vi.get_height();

        let frame_rate = vi.get_frame_rate();
        println!("Frame rate: {frame_rate:?}");

        let Some(mut clipinfo_owned) = vapoursynth::OwnedMap::create_map() else {
            return Err(LoadError::VsAllocError);
        };
        let clipinfo = clipinfo_owned.as_mut();
        script.get_variable(c_string(KF_KEY), clipinfo);
        script.get_variable(c_string(TC_KEY), clipinfo);
        script.get_variable(c_string(AUDIO_KEY), clipinfo);

        let num_kf = clipinfo
            .as_const()
            .num_elements(c_string(KF_KEY).as_c_str());
        let num_tc = clipinfo
            .as_const()
            .num_elements(c_string(TC_KEY).as_c_str());
        let has_audio = match clipinfo
            .as_const()
            .get_int(c_string(AUDIO_KEY).as_c_str(), 0)
        {
            Ok(val) => val != 0,
            Err(_) => false,
        };

        // TODO: keyframes and timecodes
        println!("num_kf: {num_kf}, num_tc: {num_tc}, has_audio: {has_audio}");

        let frame = match node.get_frame(0) {
            Ok(frame) => frame,
            Err(str) => return Err(LoadError::NoFrames(str)),
        };
        let Some(props) = frame.get_properties_ro() else {
            return Err(LoadError::FramePropertiesFailed);
        };

        #[allow(clippy::cast_precision_loss)]
        let dar = match props.get_int(c_string("_SARNum").as_c_str(), 0) {
            Ok(sar_numerator) => match props.get_int(c_string("_SARDen").as_c_str(), 0) {
                Ok(sar_denominator) => {
                    (i64::from(vi.get_width()) * sar_numerator) as f64
                        / (i64::from(vi.get_height()) * sar_denominator) as f64
                }
                Err(_) => 0.0,
            },
            Err(_) => 0.0,
        };

        println!("dar = {dar}");

        let colour_space = vapoursynth::color_matrix_description(&vi, &props);
        println!("Colour space: {colour_space}");

        let out_node = if vi.is_rgb24() {
            node
        } else {
            match Self::convert_to_rgb24(&script, &node, &vi, &props) {
                Ok(value) => value,
                Err(value) => {
                    return Err(value);
                }
            }
        };

        Ok(Video {
            _script: script,
            node: out_node,
            metadata: Metadata {
                frame_rate,
                width,
                height,
            },
        })
    }

    fn convert_to_rgb24(
        script: &vapoursynth::Script,
        node: &vapoursynth::Node,
        vi: &vapoursynth::VideoInfo,
        props: &vapoursynth::ConstMap,
    ) -> Result<vapoursynth::Node, LoadError> {
        let Some(mut resize) = script.get_core().get_resize_plugin() else {
            return Err(LoadError::ResizePluginUnavailable);
        };

        let Some(mut args_owned) = vapoursynth::OwnedMap::create_map() else {
            return Err(LoadError::VsAllocError);
        };
        let args = args_owned.as_mut();
        vapoursynth::init_resize(vi, args, props);
        args.append_node(c_string("clip").as_c_str(), node);

        let mut result_owned = resize.invoke(c_string("Bicubic").as_c_str(), args.as_const());
        let result = result_owned.as_mut();

        if let Some(err) = result.as_const().get_error() {
            return Err(LoadError::ColourSpaceConversionFailedDetail(err));
        }

        let new_node = match result.as_const().get_node(c_string("clip").as_c_str(), 0) {
            Ok(new_node) => new_node,
            Err(error_code) => {
                return Err(LoadError::ColourSpaceConversionFailedDetail(format!(
                    "[get_node] error code: {error_code}"
                )));
            }
        };

        let new_frame = match new_node.get_frame(0) {
            Ok(new_frame) => new_frame,
            Err(err) => return Err(LoadError::ColourSpaceConversionFailedDetail(err)),
        };

        let Some(new_video_info) = new_node.get_video_info() else {
            return Err(LoadError::ColourSpaceConversionFailed);
        };
        let Some(new_properties) = new_frame.get_properties_ro() else {
            return Err(LoadError::ColourSpaceConversionFailed);
        };
        let new_color_space =
            vapoursynth::color_matrix_description(&new_video_info, &new_properties);
        println!("New color space: {new_color_space}");
        Ok(new_node)
    }

    fn get_frame_internal(&self, n: model::FrameNumber) -> vapoursynth::Frame {
        let vs_frame = self.node.get_frame(n.0).unwrap();

        let video_format = vs_frame.get_video_format().unwrap();
        assert!(video_format.is_rgb24(), "Frame is not in RGB24 format");

        vs_frame
    }

    /// Retrieves the `n`th frame and returns it in `iced`'s format.
    ///
    /// # Panics
    /// Panics if the frame could not be retrieved.
    #[must_use]
    pub fn get_iced_frame(&self, n: model::FrameNumber) -> iced::widget::image::Handle {
        let instant = std::time::Instant::now();
        let vs_frame = self.get_frame_internal(n);
        let elapsed_obtain = instant.elapsed();

        let width: u32 = vs_frame
            .get_width(0)
            .try_into()
            .expect("frame width should not be negative");
        let height: u32 = vs_frame
            .get_height(0)
            .try_into()
            .expect("frame height should not be negative");

        let pitch = width as usize * 4;
        let out_len = pitch * height as usize;
        let mut out = vec![0; out_len];

        let instant2 = std::time::Instant::now();

        // Use libp2p, which we are linking to anyway because of BestSource,
        // for high performance (SIMD) packing
        #[allow(clippy::cast_possible_wrap)] // frame stride is guaranteed to fit into an `isize`
        let src_stride = [
            vs_frame.get_stride(0) as isize,
            vs_frame.get_stride(1) as isize,
            vs_frame.get_stride(2) as isize,
            0,
        ];
        let dst_stride = [pitch.try_into().expect("pitch overflow"), 0, 0, 0];
        let p2p_params = bestsource_sys::p2p_buffer_param {
            src: [
                vs_frame.get_read_ptr(0).as_ptr().cast::<libc::c_void>(),
                vs_frame.get_read_ptr(1).as_ptr().cast::<libc::c_void>(),
                vs_frame.get_read_ptr(2).as_ptr().cast::<libc::c_void>(),
                std::ptr::null(),
            ],
            dst: [
                out.as_mut_ptr().cast::<libc::c_void>(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ],
            src_stride,
            dst_stride,
            width,
            height,
            packing: bestsource_sys::p2p_packing_p2p_rgba32_be,
        };

        unsafe {
            bestsource_sys::p2p_pack_frame(
                &p2p_params,
                u64::from(bestsource_sys::P2P_ALPHA_SET_ONE),
            );
        }

        let elapsed_copy = instant2.elapsed();
        println!(
            "Frame profiling [display]: obtaining frame {n:?} took {elapsed_obtain:.2?}, packing it took {elapsed_copy:.2?}",
        );

        iced::widget::image::Handle::from_pixels(width, height, out)
    }

    /// Get a patch (monochrome region) of frame #`n` with the bounds given by the `request`.
    ///
    /// # Panics
    /// Panics if the frame could not be obtained.
    #[must_use]
    pub fn get_libmv_patch(
        &self,
        n: model::FrameNumber,
        request: super::motion::PatchRequest,
    ) -> super::motion::PatchResponse {
        // The conversion coefficients used by Blender, divided by 255
        const GREYSCALE_COEFFICIENTS: [f32; 3] = [0.000_833_373, 0.002_804_71, 0.000_283_14];

        let instant = std::time::Instant::now();
        let vs_frame = self.get_frame_internal(n);
        let elapsed_obtain = instant.elapsed();
        let frame_width: u32 = vs_frame
            .get_width(0)
            .try_into()
            .expect("frame width should not be negative");
        let frame_height: u32 = vs_frame
            .get_height(0)
            .try_into()
            .expect("frame height should not be negative");

        // Fit request parameters into the frame bounds
        #[allow(clippy::cast_sign_loss)] // we clamp to >0.0 it will never be negative
        #[allow(clippy::cast_possible_truncation)]
        let (left_within_frame, top_within_frame) = (
            request.left.clamp(0.0, f64::from(frame_width)).floor() as u32,
            request.top.clamp(0.0, f64::from(frame_height)).floor() as u32,
        );

        #[allow(clippy::cast_sign_loss)]
        #[allow(clippy::cast_possible_truncation)]
        let true_width = request
            .width
            .clamp(0.0, f64::from(frame_width - left_within_frame))
            .ceil() as u32;

        #[allow(clippy::cast_sign_loss)]
        #[allow(clippy::cast_possible_truncation)]
        let true_height = request
            .height
            .clamp(0.0, f64::from(frame_height - top_within_frame))
            .ceil() as u32;

        assert!(left_within_frame + true_width <= frame_width);
        assert!(top_within_frame + true_height <= frame_height);

        let mut out = vec![0.0_f32; true_width as usize * true_height as usize];

        let instant2 = std::time::Instant::now();

        // Assumes all frames are the same size. They should be.
        for plane in 0u8..3u8 {
            let stride = vs_frame.get_stride(i32::from(plane));
            let read_ptr = vs_frame.get_read_ptr(i32::from(plane));

            let coefficient = GREYSCALE_COEFFICIENTS[plane as usize];

            for row in 0..(true_height as usize) {
                let row_start_read =
                    stride * (top_within_frame as usize + row) + left_within_frame as usize;
                let row_read_ptr =
                    &read_ptr[row_start_read..(row_start_read + true_width as usize)];

                let row_start_write = true_width as usize * row;
                let row_write_ptr =
                    &mut out[row_start_write..(row_start_write + true_width as usize)];

                for col in 0..(true_width as usize) {
                    row_write_ptr[col] += coefficient * f32::from(row_read_ptr[col]);
                }
            }
        }

        let elapsed_copy = instant2.elapsed();
        println!(
            "Frame profiling [motion tracking]: obtaining frame {n:?} took {elapsed_obtain:.2?}, converting it took {elapsed_copy:.2?}"
        );

        super::motion::PatchResponse {
            data: out,
            left: left_within_frame,
            top: top_within_frame,
            width: true_width,
            height: true_height,
        }
    }
}

#[derive(Error, Debug)]
pub enum LoadError {
    #[error("Failed to open file (file does not exist, or its format is unsupported)")]
    FailedToOpen,

    #[error("Failed to read output node of Vapoursynth script")]
    FailedToGetNode,

    #[error("File opened is not a video file")]
    NotAVideo,

    #[error("Video format is not constant")]
    NonConstantFormat,

    #[error("Failed to create Vapoursynth data structures")]
    VsAllocError,

    #[error("Failed to get first frame from video ({0})")]
    NoFrames(String),

    #[error("Failed to get properties of first frame")]
    FramePropertiesFailed,

    #[error("Vapoursynth resize plugin is unavailable, please install it")]
    ResizePluginUnavailable,

    #[error("Colour space conversion failed ({0})")]
    ColourSpaceConversionFailedDetail(String),

    #[error("Colour space conversion failed")]
    ColourSpaceConversionFailed,
}
