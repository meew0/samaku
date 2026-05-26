pub use mv::MotionModel as Model;
pub use mv::Point;
pub use mv::Region;
use std::collections::{BTreeMap, HashMap};

use crate::model;

use super::bindings::mv;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TrackId(u64);

#[derive(Debug)]
pub struct TrackList {
    map: HashMap<TrackId, Track>,
    next_id: TrackId,
}

#[derive(Debug, Clone)]
pub struct Track {
    markers: BTreeMap<model::FrameNumber, Marker>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Marker {
    region: Region,
    key_state: KeyState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum KeyState {
    /// Manually set/edited marker which acts as the source of truth.
    Key,

    /// Marker tracked by motion tracking from a Key marker.
    Tracked,
}

#[derive(Debug, Clone, Copy)]
pub struct TrackSettings {
    model: Model,
    match_mode: MatchMode,
    pre_pass: bool,
    normalize: bool,
    channels: Channels,
}

impl Default for TrackSettings {
    fn default() -> Self {
        Self {
            model: Model::Translation,
            match_mode: MatchMode::Key,
            pre_pass: true,
            normalize: false,
            channels: Channels::rgb(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    /// Match region content to that of the key marker.
    Key,

    /// Match region content to that of the marker in the previous frame.
    Previous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channels {
    red: bool,
    green: bool,
    blue: bool,
}

impl Channels {
    fn rgb() -> Self {
        Self {
            red: true,
            green: true,
            blue: true,
        }
    }
}

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
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

pub struct Tracker<'a, V> {
    video: &'a V,
    patch_provider: fn(&V, model::FrameNumber, PatchRequest) -> PatchResponse,
    search_radius: f64,
    track: Vec<Region>,
    last_frame: model::FrameNumber,
    end_frame: model::FrameNumber,
}

impl<'a, V> Tracker<'a, V> {
    /// Create a new `MotionTracker`.
    /// The `initial_marker` should be an axis-aligned rectangle.
    /// `search_radius` is defined around the center of the `initial_marker`.
    /// `start_frame`: the frame at which the `initial_marker` is at the correct position.
    /// `end_frame`: the last frame onto which the `initial_marker` will be tracked.
    /// `track` will be of size `end_frame - start_frame + 1`, if all goes well.
    pub fn new(
        video: &'a V,
        patch_provider: fn(&V, model::FrameNumber, PatchRequest) -> PatchResponse,
        initial_marker: Region,
        search_radius: f64,
        start_frame: model::FrameNumber,
        end_frame: model::FrameNumber,
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

    #[must_use]
    pub fn track(&self) -> &Vec<Region> {
        &self.track
    }

    #[must_use]
    pub fn last_tracked_frame(&self) -> model::FrameNumber {
        self.last_frame
    }

    #[expect(
        clippy::missing_panics_doc,
        reason = "the expectation should always be met"
    )]
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

        let patch_response_1 = (self.patch_provider)(self.video, self.last_frame, patch_request);
        let patch_response_2 = (self.patch_provider)(
            self.video,
            self.last_frame + model::FrameDelta(1),
            patch_request,
        );

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
        let region1 = last_region.offset(
            -f64::from(patch_response_1.left),
            -f64::from(patch_response_1.top),
        );
        let predicted_region2 = last_region.offset(
            -f64::from(patch_response_2.left),
            -f64::from(patch_response_2.top),
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
                self.track.push(refined_region2.offset(
                    f64::from(patch_response_2.left),
                    f64::from(patch_response_2.top),
                ));
                self.last_frame += model::FrameDelta(1);
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
    fn motion_track() -> anyhow::Result<()> {
        crate::media::init();

        let path = crate::test_utils::test_file("test_files/cube_av1.mkv");

        let index = video::Video::create_indexer(&path)?.run()?;
        let video = video::Video::load(&path, index).expect("should load video");

        let initial_marker = Region::from_center_and_radius(Point { x: 272.0, y: 81.0 }, 10.0);
        let mut tracker = Tracker::new(
            &video,
            video::Video::get_libmv_patch,
            initial_marker,
            60.0,
            model::FrameNumber(0),
            model::FrameNumber(99),
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

        Ok(())
    }
}
