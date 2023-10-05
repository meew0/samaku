pub use mv::MotionModel as Model;
pub use mv::Point;
pub use mv::Region;

use super::bindings::mv;

#[derive(Debug, Clone, Copy)]
pub struct PatchRequest {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct PatchResponse {
    pub data: Vec<f32>,
    pub left: usize,
    pub top: usize,
    pub width: usize,
    pub height: usize,
}

pub struct Tracker<V, P>
where
    P: FnMut(&V, i32, PatchRequest) -> PatchResponse,
{
    video: V,
    patch_provider: P,
    search_radius: f64,
    track: Vec<Region>,
    last_frame: i32,
    end_frame: i32,
}

impl<V, P> Tracker<V, P>
where
    P: FnMut(&V, i32, PatchRequest) -> PatchResponse,
{
    /// Create a new `MotionTracker`.
    /// The `initial_marker` should be an axis-aligned rectangle.
    /// `search_radius` is defined around the center of the `initial_marker`.
    /// `start_frame`: the frame at which the `initial_marker` is at the correct position.
    /// `end_frame`: the last frame onto which the `initial_marker` will be tracked.
    /// `track` will be of size `end_frame - start_frame + 1`, if all goes well.
    pub fn new(
        video: V,
        patch_provider: P,
        initial_marker: Region,
        search_radius: f64,
        start_frame: i32,
        end_frame: i32,
    ) -> Self {
        Self {
            video,
            patch_provider,
            search_radius,
            track: vec![initial_marker],
            last_frame: start_frame,
            end_frame,
        }
    }

    pub fn track(&self) -> &Vec<Region> {
        &self.track
    }

    pub fn update(&mut self, motion_model: Model) -> TrackResult {
        if self.last_frame == self.end_frame {
            return TrackResult::Termination;
        }

        let last_region = self
            .track
            .last()
            .expect("there should be at least one region in the track");

        let patch_request = PatchRequest {
            left: last_region.center.x - self.search_radius,
            top: last_region.center.y - self.search_radius,
            width: 2.0 * self.search_radius,
            height: 2.0 * self.search_radius,
        };

        let patch_response_1 = (self.patch_provider)(&self.video, self.last_frame, patch_request);
        let patch_response_2 =
            (self.patch_provider)(&self.video, self.last_frame + 1, patch_request);

        let image1 = mv::MonochromeImage::new(
            patch_response_1.data.as_slice(),
            patch_response_1.width.try_into().unwrap(),
            patch_response_1.height.try_into().unwrap(),
        );
        let image2 = mv::MonochromeImage::new(
            patch_response_2.data.as_slice(),
            patch_response_2.width.try_into().unwrap(),
            patch_response_2.height.try_into().unwrap(),
        );

        // In theory, the two different patch responses might have different origin points,
        // because the frames might be of a different size.
        #[allow(clippy::cast_precision_loss)]
        let region1 = last_region.offset(
            -(patch_response_1.left as f64),
            -(patch_response_1.top as f64),
        );
        #[allow(clippy::cast_precision_loss)]
        let predicted_region2 = last_region.offset(
            -(patch_response_2.left as f64),
            -(patch_response_2.top as f64),
        );

        let options = mv::TrackRegionOptions {
            direction: mv::TrackRegionDirection::Forward,
            motion_model,
            num_iterations: 50,
            use_brute: true,
            use_normalization: false,
            minimum_correlation: 0.75,
            sigma: 0.9,
            image1_mask: None,
        };

        let result = mv::track_region(&options, &image1, &image2, &region1, &predicted_region2);

        match result {
            Some(refined_region2) => {
                #[allow(clippy::cast_precision_loss)]
                self.track.push(
                    refined_region2
                        .offset(patch_response_2.left as f64, patch_response_2.top as f64),
                );
                self.last_frame += 1;
                TrackResult::Success
            }
            None => TrackResult::Failure,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackResult {
    Failure,
    Termination,
    Success,
}

#[cfg(test)]
mod tests {
    use super::super::video;
    use super::*;

    #[test]
    fn motion_track() {
        let video = video::Video::load(crate::test_utils::test_file("test_files/cube_av1.mkv"));

        let initial_marker = Region::from_center_and_radius(Point { x: 272.0, y: 81.0 }, 10.0);
        let mut tracker = Tracker::new(
            video,
            video::Video::get_libmv_patch,
            initial_marker,
            60.0,
            0,
            99,
        );

        let mut last_result = TrackResult::Success;
        while last_result == TrackResult::Success {
            last_result = tracker.update(Model::Translation);
        }

        assert_eq!(last_result, TrackResult::Termination);
        assert_eq!(tracker.track().len(), 100);
        let last_region = tracker.track().last().unwrap();
        println!("{last_region:?}");
        assert!((last_region.center.x - 45.0).abs() < 2.0);
        assert!((last_region.center.y - 81.0).abs() < 2.0);
    }
}
