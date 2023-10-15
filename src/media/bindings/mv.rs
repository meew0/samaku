#![allow(dead_code)]

use std::ffi::CString;

use libmv_capi_sys as libmv;
use libmv_capi_sys::libmv_TrackRegionOptions;

pub fn init_logging(executable_name: &str) {
    let c_string =
        CString::new(executable_name).expect("`executable_name` should not contain 0 bytes");
    unsafe { libmv::libmv_initLogging(c_string.into_raw()) }
}

pub fn start_debug_logging() {
    unsafe { libmv::libmv_startDebugLogging() }
}

pub fn set_logging_verbosity(verbosity: i32) {
    unsafe { libmv::libmv_setLoggingVerbosity(verbosity) }
}

pub struct TrackRegionOptions {
    pub direction: TrackRegionDirection,
    pub motion_model: MotionModel,

    /// `[libmv]` Maximum number of Ceres iterations to run for the inner minimization.
    pub num_iterations: i32,

    /// `[libmv]` If true, apply a brute-force translation-only search before attempting the
    /// full search. This is not enabled if the destination image ("image2") is
    /// too small; in that case either the basin of attraction is close enough
    /// that the nearby minima is correct, or the search area is too small.
    pub use_brute: bool,

    /// `[libmv]` If true, normalize the image patches by their mean before doing the sum of
    /// squared error calculation. This is reasonable since the effect of
    /// increasing light intensity is multiplicative on the pixel intensities.
    ///
    /// Note: This does nearly double the solving time, so it is not advised to
    /// turn this on all the time.
    pub use_normalization: bool,

    /// `[libmv]` Minimum normalized cross-correlation necessary between the final tracked
    /// position of the patch on the destination image and the reference patch
    /// needed to declare tracking success. If the minimum correlation is not met,
    /// then TrackResult::termination is INSUFFICIENT_CORRELATION.
    pub minimum_correlation: f64,

    /// `[libmv]` The size in pixels of the blur kernel used to both smooth the image and
    /// take the image derivative.
    pub sigma: f64,

    /// `[libmv]` If non-null, this is used as the pattern mask. It should match the size of
    /// image1, even though only values inside the image1 quad are examined. The
    /// values must be in the range 0.0 to 0.1.
    pub image1_mask: Option<Vec<f32>>,
}

pub enum TrackRegionDirection {
    Forward,
    Backward,
}

impl TrackRegionDirection {
    fn as_libmv(&self) -> libmv::libmv_TrackRegionDirection {
        match self {
            TrackRegionDirection::Forward => {
                libmv::libmv_TrackRegionDirection_LIBMV_TRACK_REGION_FORWARD
            }
            TrackRegionDirection::Backward => {
                libmv::libmv_TrackRegionDirection_LIBMV_TRACK_REGION_BACKWARD
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MotionModel {
    Translation = 0,
    TranslationRotation = 1,
    TranslationScale = 2,
    TranslationRotationScale = 3,
    Affine = 4,
    Homography = 5,
}

pub struct MonochromeImage<'a> {
    data: &'a [f32],
    width: i32,
    height: i32,
}

impl<'a> MonochromeImage<'a> {
    pub fn new(data: &'a [f32], width: i32, height: i32) -> Self {
        assert_eq!(data.len(), (width * height).try_into().unwrap());
        Self {
            data,
            width,
            height,
        }
    }
}

/// Indexed in pixels from the top left, may have fractional precision.
#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    #[must_use]
    pub fn offset(&self, x_offset: f64, y_offset: f64) -> Self {
        Self {
            x: self.x + x_offset,
            y: self.y + y_offset,
        }
    }
}

/// Essentially what is shown as a “marker” in Blender's motion tracking UI,
/// with four corners and a center.
#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub top_left: Point,
    pub top_right: Point,
    pub bottom_right: Point,
    pub bottom_left: Point,
    pub center: Point,
}

impl Region {
    #[must_use]
    pub fn from_center_and_radius(center: Point, radius: f64) -> Self {
        Self {
            top_left: Point {
                x: center.x - radius,
                y: center.y - radius,
            },
            top_right: Point {
                x: center.x + radius,
                y: center.y - radius,
            },
            bottom_right: Point {
                x: center.x + radius,
                y: center.y + radius,
            },
            bottom_left: Point {
                x: center.x - radius,
                y: center.y + radius,
            },
            center,
        }
    }

    fn from_float_slices(x: &[f64; 5], y: &[f64; 5]) -> Self {
        Self {
            top_left: Point { x: x[0], y: y[0] },
            top_right: Point { x: x[1], y: y[1] },
            bottom_right: Point { x: x[2], y: y[2] },
            bottom_left: Point { x: x[3], y: y[3] },
            center: Point { x: x[4], y: y[4] },
        }
    }

    fn as_float_slices(&self) -> ([f64; 5], [f64; 5]) {
        let x = [
            self.top_left.x,
            self.top_right.x,
            self.bottom_right.x,
            self.bottom_left.x,
            self.center.x,
        ];
        let y = [
            self.top_left.y,
            self.top_right.y,
            self.bottom_right.y,
            self.bottom_left.y,
            self.center.y,
        ];
        (x, y)
    }

    #[must_use]
    pub fn offset(&self, x_offset: f64, y_offset: f64) -> Self {
        Self {
            top_left: self.top_left.offset(x_offset, y_offset),
            top_right: self.top_right.offset(x_offset, y_offset),
            bottom_right: self.bottom_right.offset(x_offset, y_offset),
            bottom_left: self.bottom_left.offset(x_offset, y_offset),
            center: self.center.offset(x_offset, y_offset),
        }
    }
}

/// Tracks the region defined by `region1` on `image1` onto `image2`.
/// Needs to be seeded with a prediction of where that region could be on `image2`,
/// this prediction is then refined and the refinement returned, if possible.
pub fn track_region(
    options: &TrackRegionOptions,
    image1: &MonochromeImage,
    image2: &MonochromeImage,
    region1: &Region,
    predicted_region2: &Region,
) -> Option<Region> {
    let image1_mask_ptr = match &options.image1_mask {
        Some(mask) => mask.as_slice().as_ptr(),
        None => std::ptr::null(),
    };

    let libmv_options = libmv_TrackRegionOptions {
        direction: options.direction.as_libmv(),
        motion_model: options.motion_model as i32,
        num_iterations: options.num_iterations,
        use_brute: i32::from(options.use_brute),
        use_normalization: i32::from(options.use_normalization),
        minimum_correlation: options.minimum_correlation,
        sigma: options.sigma,
        image1_mask: image1_mask_ptr.cast_mut(),
    };

    let (x1, y1) = region1.as_float_slices();
    let (mut x2, mut y2) = predicted_region2.as_float_slices();

    let result = unsafe {
        libmv::libmv_trackRegion(
            std::ptr::addr_of!(libmv_options),
            image1.data.as_ptr(),
            image1.width,
            image1.height,
            image2.data.as_ptr(),
            image2.width,
            image2.height,
            x1.as_ptr(),
            y1.as_ptr(),
            std::ptr::null_mut(), // argument is not used by libmv
            x2.as_mut_ptr(),
            y2.as_mut_ptr(),
        )
    };

    if result == 0 {
        None
    } else {
        Some(Region::from_float_slices(&x2, &y2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns a black image with a 5x3 white square at the given position.
    fn test_image_data(x: usize, y: usize) -> Vec<f32> {
        let mut data = vec![0.0; 300 * 200];

        for i in 0..15 {
            let current_x = x + i % 5;
            let current_y = y + i / 5;
            data[current_y * 300 + current_x] = 1.0;
        }

        data
    }

    #[test]
    fn region_track() {
        let i1_data = test_image_data(100, 100);
        let i2_data = test_image_data(102, 103);

        let image1 = MonochromeImage {
            data: i1_data.as_slice(),
            width: 300,
            height: 200,
        };
        let image2 = MonochromeImage {
            data: i2_data.as_slice(),
            width: 300,
            height: 200,
        };

        let region1 = Region::from_center_and_radius(Point { x: 100.0, y: 100.0 }, 10.0);
        let region2 = region1;

        // These appear to be the default settings used by Blender
        let options = TrackRegionOptions {
            direction: TrackRegionDirection::Forward,
            motion_model: MotionModel::Translation,
            num_iterations: 50,
            use_brute: true,
            use_normalization: false,
            minimum_correlation: 0.75,
            sigma: 0.9,
            image1_mask: None,
        };

        let result = track_region(&options, &image1, &image2, &region1, &region2);

        let refined_region2 = result.expect("tracking should have succeeded");
        assert!((refined_region2.center.x - 102.0).abs() < 0.5);
        assert!((refined_region2.center.y - 103.0).abs() < 0.5);
    }
}
