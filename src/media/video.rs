pub use super::bindings::ffms2::FrameRate;
use anyhow::Context as _;
use std::path::Path;

use crate::{model, subtitle};

use super::bindings::ffms2;
use super::index;

#[derive(Debug, Clone)]
pub struct Metadata {
    pub frame_rate: FrameRate,
    pub width: i32,
    pub height: i32,
    pub num_frames: model::FrameNumber,
    pub duration: subtitle::Duration,
}

static PIXEL_FORMAT: std::sync::LazyLock<ffms2::PixelFormat> =
    std::sync::LazyLock::new(|| ffms2::PixelFormat::from_name("rgba"));

pub struct Video {
    source: ffms2::VideoSource,
    pub metadata: Metadata,
}

impl Video {
    pub fn create_indexer<P: AsRef<Path>>(filename: P) -> anyhow::Result<index::Indexer> {
        let mut indexer = ffms2::Indexer::new(filename.as_ref()).context("creating indexer")?;
        indexer.set_track_type_index_settings(ffms2::TrackType::Video, 1);
        Ok(index::Indexer::new(indexer))
    }

    /// Load the video from the given file using FFMS2.
    pub fn load<P: AsRef<Path>>(filename: P, index: index::Index) -> anyhow::Result<Video> {
        let mut ffms_index = index.into_inner();

        let first_video_track = ffms_index
            .first_track_of_type(ffms2::TrackType::Video)
            .context("finding first video track")?;

        let mut source = ffms2::VideoSource::new(
            filename.as_ref(),
            first_video_track,
            &ffms_index,
            -1,
            ffms2::SeekMode::Normal,
        )
        .context("Failed to create video source")?;

        let first_frame = source.get_frame(0).context("Failed to get first frame")?;

        let width = first_frame.width();
        let height = first_frame.height();

        let frame_rate = source.properties.frame_rate;
        println!("Frame rate: {frame_rate:?}");

        // TODO: keyframes and timecodes
        // println!("num_kf: {num_kf}, num_tc: {num_tc}, has_audio: {has_audio}");

        // TODO: anamorphic video
        let dar = f64::from(width * source.properties.sar_numerator)
            / f64::from(height * source.properties.sar_denominator);
        println!("dar = {dar}");

        // TODO: color spaces
        let color_space = first_frame.color_space();
        println!("Color space: {color_space}");

        let num_frames = source.properties.num_frames;

        // TODO handle videos with a delay (source.properties.first_time != 0)
        // (currently the entire project assumes videos always start at 0)
        let float_duration_secs = source.properties.last_end_time;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "unavoidable precision loss"
        )]
        let duration = subtitle::Duration((float_duration_secs * 1000.0).floor() as i64);

        source
            .set_output_format(*PIXEL_FORMAT, width, height, ffms2::Resizer::Bicubic)
            .context("setting video output format")?;

        Ok(Video {
            source,
            metadata: Metadata {
                frame_rate,
                width,
                height,
                num_frames: model::FrameNumber(num_frames),
                duration,
            },
        })
    }

    fn get_frame_internal(
        &self,
        n: model::FrameNumber,
    ) -> anyhow::Result<(glam::UVec2, ffms2::Frame)> {
        let ffms_frame = self.source.get_frame(n.0)?;
        anyhow::ensure!(
            ffms_frame.pixel_format() == *PIXEL_FORMAT,
            "Frame is not in RGBA format"
        );

        let size: glam::UVec2 = ffms_frame
            .size()
            .try_into()
            .expect("frame size should not be negative");

        Ok((size, ffms_frame))
    }

    /// Retrieves the `n`th frame and returns it in `iced`'s format.
    pub fn get_iced_frame(
        &self,
        n: model::FrameNumber,
    ) -> anyhow::Result<iced::widget::image::Handle> {
        let instant = std::time::Instant::now();
        let (size, ffms_frame) = self.get_frame_internal(n)?;
        let elapsed_obtain = instant.elapsed();

        let out_len = size.x as usize * size.y as usize * 4;
        let mut out = vec![0; out_len];

        let instant2 = std::time::Instant::now();

        ffms_frame.copy_plane(0, out.as_mut_slice(), None, None, 0, 0, 2, 2, |dst, src| {
            dst.copy_from_slice(src);
        });

        let elapsed_copy = instant2.elapsed();
        println!(
            "Frame profiling [display]: obtaining frame {n:?} took {elapsed_obtain:.2?}, packing it took {elapsed_copy:.2?}",
        );

        Ok(iced::widget::image::Handle::from_rgba(size.x, size.y, out))
    }

    /// Get a patch (monochrome region) of frame #`n` with the bounds given by the `request`.
    ///
    /// # Panics
    /// Panics if the frame could not be obtained.
    pub fn get_libmv_patch(
        &self,
        n: model::FrameNumber,
        request: super::motion::PatchRequest,
    ) -> super::motion::PatchResponse {
        // The conversion coefficients used by Blender, divided by 255
        const GREYSCALE_COEFFICIENTS: [f32; 3] = [0.000_833_373, 0.002_804_71, 0.000_283_14];

        let instant = std::time::Instant::now();
        let (size, ffms_frame) = self.get_frame_internal(n).unwrap(); // TODO proper error handling
        let elapsed_obtain = instant.elapsed();

        // Fit request parameters into the frame bounds
        let origin_within_frame = request
            .origin
            .clamp(glam::DVec2::ZERO, glam::DVec2::from(size))
            .floor()
            .as_uvec2();

        let true_size = request
            .size
            .clamp(
                glam::DVec2::ZERO,
                glam::DVec2::from(size - origin_within_frame),
            )
            .ceil()
            .as_uvec2();

        assert!(
            origin_within_frame.x + true_size.x <= size.x,
            "right side of clamped patch request should fit within horizontal bounds"
        );
        assert!(
            origin_within_frame.y + true_size.y <= size.y,
            "bottom side of clamped patch request should fit within vertical bounds"
        );

        let mut out = vec![0.0_f32; true_size.x as usize * true_size.y as usize];

        let instant2 = std::time::Instant::now();

        // Assumes all frames are the same size. They should be.
        #[expect(clippy::cast_possible_wrap, reason = "64 bit only")]
        ffms_frame.copy_plane(
            0,
            &mut out,
            Some(true_size.x as usize),
            Some(true_size.y as usize),
            origin_within_frame.x as isize,
            origin_within_frame.y as isize,
            2,
            0,
            |dst, src| {
                for (i, dst_val) in dst.iter_mut().enumerate() {
                    let src_i = i << 2;
                    *dst_val = GREYSCALE_COEFFICIENTS[0].mul_add(
                        f32::from(src[src_i]),
                        GREYSCALE_COEFFICIENTS[1].mul_add(
                            f32::from(src[src_i + 1]),
                            GREYSCALE_COEFFICIENTS[2] * f32::from(src[src_i + 2]),
                        ),
                    );
                }
            },
        );

        let elapsed_copy = instant2.elapsed();
        println!(
            "Frame profiling [motion tracking]: obtaining frame {n:?} took {elapsed_obtain:.2?}, converting it took {elapsed_copy:.2?}"
        );

        super::motion::PatchResponse {
            data: out,
            origin: origin_within_frame,
            size: true_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_frame_rgba(video: &Video, n: i32) -> Vec<u8> {
        let (size, ffms_frame) = video.get_frame_internal(model::FrameNumber(n)).unwrap();
        let mut out = vec![0_u8; size.x as usize * size.y as usize * 4];
        ffms_frame.copy_plane(0, out.as_mut_slice(), None, None, 0, 0, 2, 2, |dst, src| {
            dst.copy_from_slice(src);
        });
        out
    }

    fn pixel_rgba(data: &[u8], width: u32, x: u32, y: u32) -> (u8, u8, u8) {
        let idx = ((y * width + x) * 4) as usize;
        (data[idx], data[idx + 1], data[idx + 2])
    }

    /// Returns true if the pixel has a dominant red channel (i.e. is "red", not grey).
    /// Uses a relative check so it works even with compressed/dark frames.
    fn is_red(red: u8, green: u8) -> bool {
        i32::from(red) - i32::from(green) > 30
    }

    #[test]
    fn cube_h264_metadata_and_colors() -> anyhow::Result<()> {
        crate::media::init();

        let path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_files/cube_h264.mkv");

        let index = Video::create_indexer(&path)?.run()?;
        let video = Video::load(&path, index).expect("should load video");

        assert_eq!(video.metadata.width, 320, "unexpected width");
        assert_eq!(video.metadata.height, 200, "unexpected height");
        assert_eq!(
            video.metadata.num_frames,
            model::FrameNumber(100),
            "unexpected frame count"
        );

        let first = get_frame_rgba(&video, 0);
        let middle = get_frame_rgba(&video, 49);
        let last = get_frame_rgba(&video, 99);

        // First frame: M1 (0,100) grey, M2 (319,100) red
        let (red, green, _) = pixel_rgba(&first, 320, 0, 100);
        assert!(
            !is_red(red, green),
            "first frame M1 should be grey, got red={red} green={green}"
        );
        let (red, green, _) = pixel_rgba(&first, 320, 319, 100);
        assert!(
            is_red(red, green),
            "first frame M2 should be red, got red={red} green={green}"
        );

        // Middle frame: both M1 and M2 grey
        let (red, green, _) = pixel_rgba(&middle, 320, 0, 100);
        assert!(
            !is_red(red, green),
            "middle frame M1 should be grey, got red={red} green={green}"
        );
        let (red, green, _) = pixel_rgba(&middle, 320, 319, 100);
        assert!(
            !is_red(red, green),
            "middle frame M2 should be grey, got red={red} green={green}"
        );

        // Last frame: M1 red, M2 grey
        let (red, green, _) = pixel_rgba(&last, 320, 0, 100);
        assert!(
            is_red(red, green),
            "last frame M1 should be red, got red={red} green={green}"
        );
        let (red, green, _) = pixel_rgba(&last, 320, 319, 100);
        assert!(
            !is_red(red, green),
            "last frame M2 should be grey, got red={red} green={green}"
        );

        Ok(())
    }
}
