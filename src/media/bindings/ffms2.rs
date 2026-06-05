#![allow(
    dead_code,
    reason = "implements more of what ffms2 does for now than is currently used in samaku"
)]

use crate::{model, subtitle};
use ffms2_sys as ffms2;
use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::fmt::{Debug, Display};
use std::pin::Pin;
use std::ptr;

pub(crate) fn init() {
    unsafe {
        ffms2::FFMS_Init(0, 0);
    }
}

pub(crate) struct Index {
    index: *mut ffms2::FFMS_Index,
    buffer: *mut u8,
    error: InternalError,
}

// TODO: there are data races in ffmpeg when running the tests in ThreadSanitizer mode
// $ RUSTFLAGS="-Z sanitizer=thread" cargo +nightly test -Z build-std --target x86_64-unknown-linux-gnu -- --test-threads=8
// Might be caused by these unsafe impls. Remains to be seen whether there are any consequences.
unsafe impl Send for Index {}

impl Index {
    pub(crate) fn new<P: AsRef<std::path::Path>>(filename: P) -> Result<Self, FfmsError> {
        let source = super::path_to_cstring(filename);
        let mut error = InternalError::allocate();
        let index = unsafe { ffms2::FFMS_ReadIndex(source.as_ptr(), error.as_mut_ptr()) };

        if index.is_null() {
            Err(error.error())
        } else {
            Ok(Self {
                index,
                buffer: ptr::null_mut(),
                error,
            })
        }
    }

    pub(crate) fn first_track_of_type(&mut self, track_type: TrackType) -> Result<i32, FfmsError> {
        let num_tracks = unsafe {
            ffms2::FFMS_GetFirstTrackOfType(self.index, track_type as i32, self.error.as_mut_ptr())
        };
        if num_tracks < 0 {
            Err(self.error.error())
        } else {
            Ok(num_tracks)
        }
    }
}

impl Drop for Index {
    fn drop(&mut self) {
        unsafe {
            if !self.buffer.is_null() {
                ffms2::FFMS_FreeIndexBuffer(&raw mut self.buffer);
            }
            ffms2::FFMS_DestroyIndex(self.index);
        }
    }
}

pub type ProgressCallback = Box<dyn FnMut(i64, i64) -> model::CancellationState + Send>;

pub(crate) struct Indexer {
    indexer: *mut ffms2::FFMS_Indexer,
    error: InternalError,
    callback: Cell<Option<Pin<Box<ProgressCallback>>>>,
}

unsafe impl Send for Indexer {}

impl Indexer {
    pub(crate) fn new<P: AsRef<std::path::Path>>(filename: P) -> Result<Self, FfmsError> {
        let source = super::path_to_cstring(filename);
        let mut error = InternalError::allocate();
        let indexer = unsafe { ffms2::FFMS_CreateIndexer(source.as_ptr(), error.as_mut_ptr()) };

        if indexer.is_null() {
            Err(error.error())
        } else {
            Ok(Self {
                indexer,
                error,
                callback: Cell::new(None),
            })
        }
    }

    pub(crate) fn set_track_type_index_settings(&mut self, track_type: TrackType, index: i32) {
        unsafe {
            ffms2::FFMS_TrackTypeIndexSettings(self.indexer, track_type as i32, index, 0);
        }
    }
    pub(crate) fn set_progress_callback<
        F: FnMut(i64, i64) -> model::CancellationState + Send + 'static,
    >(
        &mut self,
        callback: F,
    ) {
        // We need the two levels of `Box` because the callback function pointer is a DST, so we
        // essentially have (thin pointer) -> (wide pointer) -> (DST)
        let ptr: *mut ProgressCallback = Box::into_raw(Box::new(Box::new(callback)));

        unsafe {
            ffms2::FFMS_SetProgressCallback(
                self.indexer,
                Some(Self::internal_callback),
                ptr.cast(),
            );
        }

        // Replace the previous callback, dropping it in the process
        let box_again = unsafe { Box::from_raw(ptr) };
        self.callback.replace(Some(Pin::new(box_again)));
    }

    unsafe extern "C" fn internal_callback(
        current: i64,
        total: i64,
        opaque_data: *mut libc::c_void,
    ) -> i32 {
        let callback_ptr: *mut ProgressCallback = opaque_data.cast();
        let callback: &mut dyn FnMut(i64, i64) -> model::CancellationState =
            unsafe { &mut **callback_ptr };

        let result = callback(current, total);

        i32::from(result.should_cancel())
    }

    pub(crate) fn do_indexing(
        mut self,
        error_handling: IndexErrorHandling,
    ) -> Result<Index, FfmsError> {
        let index = unsafe {
            ffms2::FFMS_DoIndexing2(
                self.indexer,
                (error_handling as u32).cast_signed(),
                self.error.as_mut_ptr(),
            )
        };

        if index.is_null() {
            Err(self.error.error())
        } else {
            let Self {
                indexer: _indexer,
                error,
                callback: _callback,
            } = self;

            Ok(Index {
                index,
                buffer: ptr::null_mut(),
                error, // recycle the error allocation from the indexer
            })
        }
    }

    pub(crate) fn cancel_indexing(self) {
        unsafe {
            ffms2::FFMS_CancelIndexing(self.indexer);
        }
    }
}

#[repr(u32)]
pub(crate) enum IndexErrorHandling {
    Abort = ffms2::FFMS_IndexErrorHandling::FFMS_IEH_ABORT as u32,
    ClearTrack = ffms2::FFMS_IndexErrorHandling::FFMS_IEH_CLEAR_TRACK as u32,
    StopTrack = ffms2::FFMS_IndexErrorHandling::FFMS_IEH_STOP_TRACK as u32,
    Ignore = ffms2::FFMS_IndexErrorHandling::FFMS_IEH_IGNORE as u32,
}

#[repr(i32)]
pub(crate) enum TrackType {
    Unknown = ffms2::FFMS_TrackType::FFMS_TYPE_UNKNOWN as i32,
    Video = ffms2::FFMS_TrackType::FFMS_TYPE_VIDEO as i32,
    Audio = ffms2::FFMS_TrackType::FFMS_TYPE_AUDIO as i32,
    Data = ffms2::FFMS_TrackType::FFMS_TYPE_DATA as i32,
    Subtitle = ffms2::FFMS_TrackType::FFMS_TYPE_SUBTITLE as i32,
    Attachment = ffms2::FFMS_TrackType::FFMS_TYPE_ATTACHMENT as i32,
}

pub(crate) struct AudioSource {
    audio_source: *mut ffms2::FFMS_AudioSource,
    pub properties: AudioProperties,
    error: InternalError,
}

unsafe impl Send for AudioSource {}

impl AudioSource {
    pub(crate) fn new<P: AsRef<std::path::Path>>(
        filename: P,
        track: i32,
        index: &Index,
        delay_mode: AudioDelayMode,
    ) -> Result<Self, FfmsError> {
        let source = super::path_to_cstring(filename);
        let mut error = InternalError::allocate();
        let audio_source = unsafe {
            ffms2::FFMS_CreateAudioSource(
                source.as_ptr(),
                track,
                index.index,
                delay_mode as i32,
                error.as_mut_ptr(),
            )
        };

        if audio_source.is_null() {
            Err(error.error())
        } else {
            let properties = match Self::get_audio_properties(audio_source) {
                Ok(properties) => properties,
                Err(inner_error) => {
                    return Err(FfmsError {
                        main_type: ErrorType::Bindings,
                        subtype: ErrorType::Unsupported,
                        message: format!("error while reading audio properties: {inner_error}"),
                    });
                }
            };

            Ok(Self {
                audio_source,
                properties,
                error,
            })
        }
    }

    fn get_audio_properties(
        audio_source: *mut ffms2::FFMS_AudioSource,
    ) -> Result<AudioProperties, UnsupportedAudioError> {
        let audio_prop = unsafe { ffms2::FFMS_GetAudioProperties(audio_source) };
        let internal_properties = unsafe { &*audio_prop };

        let properties = AudioProperties {
            channels: internal_properties
                .Channels
                .try_into()
                .map_err(UnsupportedAudioError::ChannelNumberOverflow)?,
            sample_rate: internal_properties
                .SampleRate
                .try_into()
                .map_err(UnsupportedAudioError::SampleRateOverflow)?,
            num_frames: internal_properties
                .NumSamples
                .try_into()
                .map_err(UnsupportedAudioError::NumSamplesUnderflow)?,
            sample_format: if internal_properties.SampleFormat
                == ffms2::FFMS_SampleFormat::FFMS_FMT_S16 as i32
            {
                cpal::SampleFormat::I16
            } else if internal_properties.SampleFormat
                == ffms2::FFMS_SampleFormat::FFMS_FMT_S32 as i32
            {
                cpal::SampleFormat::I32
            } else if internal_properties.SampleFormat
                == ffms2::FFMS_SampleFormat::FFMS_FMT_U8 as i32
            {
                cpal::SampleFormat::U8
            } else if internal_properties.SampleFormat
                == ffms2::FFMS_SampleFormat::FFMS_FMT_FLT as i32
            {
                cpal::SampleFormat::F32
            } else if internal_properties.SampleFormat
                == ffms2::FFMS_SampleFormat::FFMS_FMT_DBL as i32
            {
                cpal::SampleFormat::F64
            } else {
                return Err(UnsupportedAudioError::InvalidSampleFormat(
                    internal_properties.SampleFormat,
                ));
            },
        };

        Ok(properties)
    }

    pub(crate) fn get_audio<T>(
        &mut self,
        start_frame: usize,
        count_frames: usize,
        buffer: &mut [T],
    ) -> Result<(), FfmsError> {
        let num_frames = self.properties.num_frames;

        if start_frame + count_frames >= num_frames {
            return Err(FfmsError {
                main_type: ErrorType::Bindings,
                subtype: ErrorType::InvalidArgument,
                message: "requesting samples beyond end of track".to_owned(),
            });
        }

        let num_channels = self.properties.channels;
        let num_samples = count_frames * num_channels as usize;

        if buffer.len() < num_samples {
            return Err(FfmsError {
                main_type: ErrorType::Bindings,
                subtype: ErrorType::InvalidArgument,
                message: "provided buffer too small".to_owned(),
            });
        }

        if size_of::<T>() != self.properties.sample_format.sample_size() {
            return Err(FfmsError {
                main_type: ErrorType::Bindings,
                subtype: ErrorType::InvalidArgument,
                message: "provided buffer of wrong type".to_owned(),
            });
        }

        let start_frame_i64: i64 = start_frame.try_into().map_err(|try_into_error| FfmsError {
            main_type: ErrorType::Bindings,
            subtype: ErrorType::InvalidArgument,
            message: format!("start_frame overflow: {try_into_error:?}"),
        })?;
        let count_frames_i64: i64 =
            count_frames
                .try_into()
                .map_err(|try_into_error| FfmsError {
                    main_type: ErrorType::Bindings,
                    subtype: ErrorType::InvalidArgument,
                    message: format!("count_frames overflow: {try_into_error:?}"),
                })?;

        let err = unsafe {
            ffms2::FFMS_GetAudio(
                self.audio_source,
                buffer.as_mut_ptr().cast::<u8>().cast(),
                start_frame_i64,
                count_frames_i64,
                self.error.as_mut_ptr(),
            )
        };

        if err != 0 {
            Err(self.error.error())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioProperties {
    pub channels: u16,
    pub sample_rate: u32,
    pub sample_format: cpal::SampleFormat,
    pub num_frames: usize,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum UnsupportedAudioError {
    #[error("channel number overflow: {0:?}")]
    ChannelNumberOverflow(std::num::TryFromIntError),

    #[error("sample rate overflow: {0:?}")]
    SampleRateOverflow(std::num::TryFromIntError),

    #[error("num samples underflow: {0:?}")]
    NumSamplesUnderflow(std::num::TryFromIntError),

    #[error("invalid sample format: {0}")]
    InvalidSampleFormat(i32),
}

#[repr(i32)]
pub(crate) enum AudioDelayMode {
    NoShift = ffms2::FFMS_AudioDelayModes::FFMS_DELAY_NO_SHIFT as i32,
    TimeZero = ffms2::FFMS_AudioDelayModes::FFMS_DELAY_TIME_ZERO as i32,
    FirstVideoTrack = ffms2::FFMS_AudioDelayModes::FFMS_DELAY_FIRST_VIDEO_TRACK as i32,
}

pub(crate) struct VideoSource {
    video_source: *mut ffms2::FFMS_VideoSource,
    pub properties: VideoProperties,
    track: *mut ffms2::FFMS_Track,
    error: RefCell<InternalError>,
}

unsafe impl Send for VideoSource {}

impl VideoSource {
    pub(crate) fn new<P: AsRef<std::path::Path>>(
        filename: P,
        track_num: i32,
        index: &Index,
        threads: i32,
        seek_mode: SeekMode,
    ) -> Result<Self, FfmsError> {
        let source = super::path_to_cstring(filename);
        let error_cell = RefCell::new(InternalError::allocate());
        let mut error = error_cell.borrow_mut();
        let video_source = unsafe {
            ffms2::FFMS_CreateVideoSource(
                source.as_ptr(),
                track_num,
                index.index,
                threads,
                seek_mode as i32,
                error.as_mut_ptr(),
            )
        };

        if video_source.is_null() {
            Err(error.error())
        } else {
            let properties = Self::get_video_properties(video_source);
            let track = unsafe { ffms2::FFMS_GetTrackFromVideo(video_source) };

            // as far as I can tell, `GetTrackFromVideo` should never fail
            assert!(
                !track.is_null(),
                "null track returned from ffms2::FFMS_GetTrackFromVideo"
            );

            drop(error);
            Ok(Self {
                video_source,
                properties,
                track,
                error: error_cell,
            })
        }
    }

    fn get_video_properties(video_source: *mut ffms2::FFMS_VideoSource) -> VideoProperties {
        let video_prop = unsafe { ffms2::FFMS_GetVideoProperties(video_source) };
        let internal_properties = unsafe { &*video_prop };

        VideoProperties {
            frame_rate: FrameRate {
                numerator: internal_properties
                    .FPSNumerator
                    .try_into()
                    .expect("negative framerate numerator"),
                denominator: internal_properties
                    .FPSDenominator
                    .try_into()
                    .expect("negative framerate denominator"),
            },
            num_frames: internal_properties.NumFrames,
            sar_numerator: internal_properties.SARNum,
            sar_denominator: internal_properties.SARDen,
            first_time: internal_properties.FirstTime,
            last_time: internal_properties.LastTime,
            last_end_time: internal_properties.LastEndTime,
        }
    }

    pub(crate) fn get_frame(&self, n: i32) -> Result<Frame, FfmsError> {
        let mut error = self.error.borrow_mut();
        let frame = unsafe { ffms2::FFMS_GetFrame(self.video_source, n, error.as_mut_ptr()) };

        if frame.is_null() {
            Err(error.error())
        } else {
            Ok(Frame { frame })
        }
    }

    pub(crate) fn get_frame_info(&self, n: i32) -> Option<FrameInfo> {
        let frame_info = unsafe { ffms2::FFMS_GetFrameInfo(self.track, n) };

        if frame_info.is_null() {
            None
        } else {
            Some(unsafe {
                FrameInfo {
                    pts: (*frame_info).PTS,
                    repeat_pict: (*frame_info).RepeatPict,
                    keyframe: (*frame_info).KeyFrame != 0,
                }
            })
        }
    }

    pub(crate) fn set_output_format(
        &mut self,
        format: PixelFormat,
        width: i32,
        height: i32,
        resizer: Resizer,
    ) -> Result<(), FfmsError> {
        let formats: [i32; 2] = [format.0, -1];

        let mut error = self.error.borrow_mut();
        let result = unsafe {
            ffms2::FFMS_SetOutputFormatV2(
                self.video_source,
                (&raw const formats).cast(),
                width,
                height,
                resizer as i32,
                error.as_mut_ptr(),
            )
        };

        if result != 0 {
            Err(error.error())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VideoProperties {
    pub frame_rate: FrameRate,
    pub num_frames: i32,
    pub sar_numerator: i32,
    pub sar_denominator: i32,
    pub first_time: f64,
    pub last_time: f64,
    pub last_end_time: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FrameRate {
    pub numerator: u64,
    pub denominator: u64,
}

impl FrameRate {
    pub const F24: FrameRate = FrameRate {
        numerator: 24,
        denominator: 1,
    };

    pub const F23_976: FrameRate = FrameRate {
        numerator: 24000,
        denominator: 1001,
    };

    /// Get the number of the closest frame before the given time point in milliseconds.
    ///
    /// # Panics
    /// Panics if the resulting frame number would not fit into an `i32`.
    #[must_use]
    pub(crate) fn ms_to_frame(&self, ass_ms: i64) -> model::FrameNumber {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "numerator is guaranteed to be smaller than i64 max"
        )]
        let numerator = ass_ms * self.numerator as i64;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "denominator is guaranteed to be smaller than i64 max"
        )]
        let denominator = 1000 * self.denominator as i64;
        model::FrameNumber(
            (numerator / denominator)
                .try_into()
                .expect("overflow while converting time to frame number"),
        )
    }

    /// Get the number of the closest frame *after* the given time point in milliseconds.
    ///
    /// # Panics
    /// Panics if the resulting frame number would not fit into an `i32`.
    #[must_use]
    pub(crate) fn ms_to_frame_after(&self, ass_ms: i64) -> model::FrameNumber {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "denominator is guaranteed to be smaller than i64 max"
        )]
        let denominator = 1000 * self.denominator as i64;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "numerator is guaranteed to be smaller than i64 max"
        )]
        let numerator = (ass_ms * self.numerator as i64) + denominator - 1;
        model::FrameNumber(
            (numerator / denominator)
                .try_into()
                .expect("overflow while converting time to frame number"),
        )
    }

    #[must_use]
    pub(crate) fn frame_to_ms(&self, frame: model::FrameNumber) -> i64 {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "denominator is guaranteed to be smaller than i64 max"
        )]
        let inv_numerator = i64::from(frame.0 * 1000) * self.denominator as i64;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "numerator is guaranteed to be smaller than i64 max"
        )]
        let result = inv_numerator / self.numerator as i64;
        result
    }

    pub(crate) fn ass_time_to_frame(&self, ass_time: subtitle::StartTime) -> model::FrameNumber {
        self.ms_to_frame(ass_time.0)
    }

    pub(crate) fn ass_time_to_frame_after(
        &self,
        ass_time: subtitle::StartTime,
    ) -> model::FrameNumber {
        self.ms_to_frame_after(ass_time.0)
    }

    pub(crate) fn frame_to_ass_time(&self, frame: model::FrameNumber) -> subtitle::StartTime {
        subtitle::StartTime(self.frame_to_ms(frame))
    }

    #[must_use]
    pub(crate) fn frame_time_ms(&self) -> i64 {
        self.frame_to_ms(model::FrameNumber(1))
    }

    pub(crate) fn iter_from(
        &self,
        frame: model::FrameNumber,
    ) -> impl Iterator<Item = (model::FrameNumber, i64)> {
        FrameIterator {
            frame_rate: self,
            current: frame,
        }
    }
}

impl From<FrameRate> for f64 {
    /// Convert the frame rate to a floating-point value by dividing the numerator by the
    /// denominator. May lose precision for very large numerators/denominators.
    #[expect(
        clippy::cast_precision_loss,
        reason = "amount of precision loss is acceptable in this case"
    )]
    fn from(value: FrameRate) -> Self {
        value.numerator as f64 / value.denominator as f64
    }
}

struct FrameIterator<'a> {
    frame_rate: &'a FrameRate,
    current: model::FrameNumber,
}

impl Iterator for FrameIterator<'_> {
    type Item = (model::FrameNumber, i64);

    fn next(&mut self) -> Option<Self::Item> {
        self.current += model::FrameDelta(1);
        Some((self.current, self.frame_rate.frame_to_ms(self.current)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelFormat(i32);

impl PixelFormat {
    pub(crate) fn from_name(name: &str) -> Self {
        let c_string = super::c_string(name);
        let format_num = unsafe { ffms2::FFMS_GetPixFmt(c_string.as_ptr()) };
        Self(format_num)
    }
}

#[repr(i32)]
pub(crate) enum SeekMode {
    LinearNoRw = ffms2::FFMS_SeekMode::FFMS_SEEK_LINEAR_NO_RW as i32,
    Linear = ffms2::FFMS_SeekMode::FFMS_SEEK_LINEAR as i32,
    Normal = ffms2::FFMS_SeekMode::FFMS_SEEK_NORMAL as i32,
    Unsafe = ffms2::FFMS_SeekMode::FFMS_SEEK_UNSAFE as i32,
    Aggressive = ffms2::FFMS_SeekMode::FFMS_SEEK_AGGRESSIVE as i32,
}

#[repr(u32)]
pub(crate) enum Resizer {
    FastBilinear = ffms2::FFMS_Resizers::FFMS_RESIZER_FAST_BILINEAR as u32,
    Bilinear = ffms2::FFMS_Resizers::FFMS_RESIZER_BILINEAR as u32,
    Bicubic = ffms2::FFMS_Resizers::FFMS_RESIZER_BICUBIC as u32,
    Experimental = ffms2::FFMS_Resizers::FFMS_RESIZER_X as u32,
    Point = ffms2::FFMS_Resizers::FFMS_RESIZER_POINT as u32,
    Area = ffms2::FFMS_Resizers::FFMS_RESIZER_AREA as u32,
    BicubLin = ffms2::FFMS_Resizers::FFMS_RESIZER_BICUBLIN as u32,
    Gauss = ffms2::FFMS_Resizers::FFMS_RESIZER_GAUSS as u32,
    Sinc = ffms2::FFMS_Resizers::FFMS_RESIZER_SINC as u32,
    Lanczos = ffms2::FFMS_Resizers::FFMS_RESIZER_LANCZOS as u32,
    Spline = ffms2::FFMS_Resizers::FFMS_RESIZER_SPLINE as u32,
}

pub(crate) struct Frame {
    frame: *const ffms2::FFMS_Frame,
}

impl Frame {
    pub(crate) fn width(&self) -> i32 {
        let scaled_width = unsafe { (*self.frame).ScaledWidth };
        if scaled_width < 0 {
            unsafe { (*self.frame).EncodedWidth }
        } else {
            scaled_width
        }
    }

    pub(crate) fn height(&self) -> i32 {
        let scaled_height = unsafe { (*self.frame).ScaledHeight };
        if scaled_height < 0 {
            unsafe { (*self.frame).EncodedHeight }
        } else {
            scaled_height
        }
    }

    pub(crate) fn size(&self) -> glam::IVec2 {
        glam::IVec2 {
            x: self.width(),
            y: self.height(),
        }
    }

    pub(crate) fn color_space(&self) -> i32 {
        unsafe { (*self.frame).ColorSpace }
    }

    pub(crate) fn pixel_format(&self) -> PixelFormat {
        let format_num = unsafe { (*self.frame).ConvertedPixelFormat };
        PixelFormat(format_num)
    }

    #[expect(clippy::too_many_arguments, reason = "it seems sensible in this case")]
    pub(crate) fn copy_plane<T: Copy>(
        &self,
        plane_index: usize,
        target: &mut [T],
        width_override: Option<usize>,
        height_override: Option<usize>,
        x_start: isize,
        y_start: isize,
        src_row_size_shift: usize,
        dst_row_size_shift: usize,
        row_assign: fn(&mut [T], &[u8]) -> (),
    ) {
        let width: usize =
            width_override.unwrap_or_else(|| self.width().try_into().expect("negative width"));
        let height: usize =
            height_override.unwrap_or_else(|| self.height().try_into().expect("negative height"));
        let linesize = unsafe { (*self.frame).Linesize[plane_index] } as isize;

        let mut ptr = unsafe { (*self.frame).Data[plane_index].offset(linesize * y_start) };
        assert!(!ptr.is_null(), "data pointer is null");

        let mut dst_row_start = 0_usize;
        let src_row_offset = x_start << src_row_size_shift;
        let src_row_len = width << src_row_size_shift;
        let dst_row_len = width << dst_row_size_shift;
        for _ in 0..height {
            let row_read_slice =
                unsafe { std::slice::from_raw_parts(ptr.offset(src_row_offset), src_row_len) };
            let dst_row_end = dst_row_start + dst_row_len;
            let row_write_slice = &mut target[dst_row_start..dst_row_end];

            row_assign(row_write_slice, row_read_slice);

            ptr = unsafe { ptr.offset(linesize) };
            dst_row_start = dst_row_end;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameInfo {
    pts: i64,
    repeat_pict: i32,
    keyframe: bool,
}

pub(crate) struct FfmsError {
    main_type: ErrorType,
    subtype: ErrorType,
    pub message: String,
}

impl Debug for FfmsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error type: {}\nSubtype: {}\nMessage: {}",
            self.main_type, self.subtype, self.message
        )
    }
}

impl Display for FfmsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

impl std::error::Error for FfmsError {}

#[derive(Debug, thiserror::Error)]
enum ErrorType {
    #[error("No error")]
    Success,

    // Main types - where the error occurred
    #[error("Index file handling error")]
    Index,
    #[error("Indexing error")]
    Indexing,
    #[error("Video postprocessing error")]
    Postprocessing,
    #[error("Image scaling error")]
    Scaling,
    #[error("Audio/video decoding error")]
    Decoding,
    #[error("Seeking error")]
    Seeking,
    #[error("File parsing error")]
    Parser,
    #[error("Track handling error")]
    Track,
    #[error("WAVE64 file writer error")]
    WaveWriter,
    #[error("Operation aborted")]
    Cancelled,
    #[error("Resampling")]
    Resampling,
    #[error("FFMS2 bindings error")]
    Bindings,

    // Subtypes - what caused the error
    #[error("Unknown error")]
    Unknown,
    #[error("Format or operation is not supported")]
    Unsupported,
    #[error("Cannot read from file")]
    FileRead,
    #[error("Cannot write to file")]
    FileWrite,
    #[error("No such file or directory")]
    NoFile,
    #[error("Wrong version")]
    Version,
    #[error("Out of memory")]
    AllocationFailed,
    #[error("Invalid or nonsensical argument")]
    InvalidArgument,
    #[error("Decoder error")]
    Codec,
    #[error("Requested mode or operation unavailable")]
    NotAvailable,
    #[error("Provided index does not match the file")]
    FileMismatch,
    #[error("User error")]
    User,
}

impl ErrorType {
    fn from_code(code: i32) -> Self {
        if code == ffms2::FFMS_Errors::FFMS_ERROR_SUCCESS as i32 {
            Self::Success
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_INDEX as i32 {
            Self::Index
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_INDEXING as i32 {
            Self::Indexing
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_POSTPROCESSING as i32 {
            Self::Postprocessing
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_SCALING as i32 {
            Self::Scaling
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_DECODING as i32 {
            Self::Decoding
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_SEEKING as i32 {
            Self::Seeking
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_PARSER as i32 {
            Self::Parser
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_TRACK as i32 {
            Self::Track
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_WAVE_WRITER as i32 {
            Self::WaveWriter
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_CANCELLED as i32 {
            Self::Cancelled
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_RESAMPLING as i32 {
            Self::Resampling
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_UNKNOWN as i32 {
            Self::Unknown
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_UNSUPPORTED as i32 {
            Self::Unsupported
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_FILE_READ as i32 {
            Self::FileRead
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_FILE_WRITE as i32 {
            Self::FileWrite
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_NO_FILE as i32 {
            Self::NoFile
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_VERSION as i32 {
            Self::Version
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_ALLOCATION_FAILED as i32 {
            Self::AllocationFailed
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_INVALID_ARGUMENT as i32 {
            Self::InvalidArgument
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_CODEC as i32 {
            Self::Codec
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_NOT_AVAILABLE as i32 {
            Self::NotAvailable
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_FILE_MISMATCH as i32 {
            Self::FileMismatch
        } else if code == ffms2::FFMS_Errors::FFMS_ERROR_USER as i32 {
            Self::User
        } else {
            panic!("Invalid error code: {code}")
        }
    }
}

struct InternalError {
    error_info: ffms2::FFMS_ErrorInfo,
    buffer: Pin<Box<[u8; InternalError::BUFFER_SIZE as usize]>>,
}

unsafe impl Send for InternalError {}

impl InternalError {
    const BUFFER_SIZE: u16 = 1024;

    pub(crate) fn allocate() -> Self {
        let mut buffer = Pin::new(Box::new([0_u8; Self::BUFFER_SIZE as usize]));
        let error_info = ffms2::FFMS_ErrorInfo {
            ErrorType: 0,
            SubType: 0,
            Buffer: buffer.as_mut().as_mut_ptr().cast(),
            BufferSize: i32::from(Self::BUFFER_SIZE),
        };

        Self { error_info, buffer }
    }

    pub(crate) fn error(&self) -> FfmsError {
        let c_str = CStr::from_bytes_until_nul(&*self.buffer)
            .expect("error while converting error message to string");
        FfmsError {
            main_type: ErrorType::from_code(self.error_info.ErrorType),
            subtype: ErrorType::from_code(self.error_info.SubType),
            message: c_str.to_string_lossy().into_owned(),
        }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut ffms2::FFMS_ErrorInfo {
        &raw mut self.error_info
    }
}
