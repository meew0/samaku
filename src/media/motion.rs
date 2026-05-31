use glam::{DVec2, UVec2};
pub use mv::MotionModel as Model;
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

impl TrackList {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn get(&self, id: TrackId) -> Option<&Track> {
        self.map.get(&id)
    }

    #[must_use]
    pub fn get_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.map.get_mut(&id)
    }

    pub fn add(&mut self, track: Track) -> TrackId {
        let id = self.next_id;
        self.next_id = TrackId(id.0 + 1);
        self.map.insert(id, track);
        id
    }

    pub fn remove(&mut self, id: TrackId) -> Option<Track> {
        self.map.remove(&id)
    }

    pub fn restore(&mut self, id: TrackId, track: Track) {
        self.map.insert(id, track);
    }

    #[must_use]
    pub fn stab(&self, frame: model::FrameNumber) -> Vec<(TrackId, &Track)> {
        // TODO: optimize this using interavl
        self.map
            .iter()
            .filter_map(|(&id, track)| {
                (frame >= track.first_frame && frame <= track.last_frame).then_some((id, track))
            })
            .collect()
    }
}

impl Default for TrackList {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            next_id: TrackId(0),
        }
    }
}

impl model::NamedListIterable for TrackList {
    type Key = TrackId;

    fn iter_named(&self) -> impl Iterator<Item = model::NamedEntry<'_, Self::Key>> {
        self.map.iter().map(|(&key, value)| model::NamedEntry {
            id: key,
            name: &value.name,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Track {
    markers: BTreeMap<model::FrameNumber, Marker>,
    pub name: String,
    first_frame: model::FrameNumber,
    last_frame: model::FrameNumber,
}

impl Track {
    #[must_use]
    pub fn new(origin_frame: model::FrameNumber, marker: Marker, name: String) -> Self {
        let mut markers = BTreeMap::new();
        markers.insert(origin_frame, marker);
        Self {
            markers,
            name,
            first_frame: origin_frame,
            last_frame: origin_frame,
        }
    }

    #[must_use]
    pub fn get_marker(&self, frame_number: model::FrameNumber) -> Option<&Marker> {
        self.markers.get(&frame_number)
    }

    #[must_use]
    pub fn get_marker_mut(&mut self, frame_number: model::FrameNumber) -> Option<&mut Marker> {
        self.markers.get_mut(&frame_number)
    }
}

impl model::Named for Track {
    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Marker {
    pub region: Region,
    pub offset: DVec2,
    pub search_area: Patch<DVec2>,
    pub key_state: KeyState,
}

impl Marker {
    /// Moves the marker by the given delta.
    /// Moves both the region and the search area.
    pub fn move_delta(&mut self, delta: DVec2) {
        self.region = self.region.offset(delta);
        self.search_area.origin += delta;
    }

    /// Update the marker region to the given new region.
    /// Moves and scales the search area so that its border around the region remains the same.
    pub fn update_region(&mut self, new_region: Region) {
        // Find the current bounding box and the padding towards the search area
        let old_bb = self.region.bounding_box();
        let pad_left = old_bb.origin.x - self.search_area.origin.x;
        let pad_top = old_bb.origin.y - self.search_area.origin.y;
        let pad_right = (self.search_area.origin.x + self.search_area.size.x)
            - (old_bb.origin.x + old_bb.size.x);
        let pad_bottom = (self.search_area.origin.y + self.search_area.size.y)
            - (old_bb.origin.y + old_bb.size.y);

        // Find the bounding box of the new region, and adjust the search area accordingly
        let new_bb = new_region.bounding_box();
        self.search_area.origin = DVec2::new(new_bb.origin.x - pad_left, new_bb.origin.y - pad_top);
        self.search_area.size = DVec2::new(
            new_bb.size.x + pad_left + pad_right,
            new_bb.size.y + pad_top + pad_bottom,
        );

        self.region = new_region;
    }

    /// Update the search area, ensuring it does not go into the bounds of the region.
    pub fn update_search_area(&mut self, new_search_area: Patch<DVec2>) {
        let old_bb = self.region.bounding_box();
        let new_origin = new_search_area.origin.min(old_bb.origin);
        let new_bottom_right =
            (new_search_area.origin + new_search_area.size).max(old_bb.origin + old_bb.size);

        self.search_area.origin = new_origin;
        self.search_area.size = new_bottom_right - new_origin;
    }
}

impl Default for Marker {
    fn default() -> Self {
        Self {
            region: Region::from_center_and_radius(DVec2::new(100.0, 100.0), 20.0),
            offset: DVec2::ZERO,
            search_area: Patch {
                origin: DVec2::new(50.0, 50.0),
                size: DVec2::new(100.0, 100.0),
            },
            key_state: KeyState::Key,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum KeyState {
    /// Manually set/edited marker which acts as the source of truth.
    Key,

    /// Marker tracked by motion tracking from a Key marker.
    Tracked,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct TrackSettings {
    pub model: Model,
    pub match_mode: MatchMode,
    pub pre_pass: bool,
    pub normalize: bool,
    pub channels: Channels,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MatchMode {
    /// Match region content to that of the key marker.
    Key,

    /// Match region content to that of the marker in the previous frame.
    Previous,
}

impl std::fmt::Display for MatchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            MatchMode::Key => {
                write!(f, "Keyframe")
            }
            MatchMode::Previous => {
                write!(f, "Previous frame")
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(usize)]
pub enum Channel {
    Red = 0,
    Green = 1,
    Blue = 2,
}

impl Channel {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Channel::Red => "Red",
            Channel::Green => "Green",
            Channel::Blue => "Blue",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

impl std::ops::Index<Channel> for Channels {
    type Output = bool;

    fn index(&self, index: Channel) -> &Self::Output {
        match index {
            Channel::Red => &self.red,
            Channel::Green => &self.green,
            Channel::Blue => &self.blue,
        }
    }
}

pub type Direction = mv::TrackRegionDirection;

#[derive(Debug, Clone, Copy)]
pub enum Target {
    /// Track until the given frame, inclusive.
    Frame(model::FrameNumber),

    /// Track as far as possible.
    None,
}

impl Target {
    #[must_use]
    pub fn event(limit_to_event: bool, event_target_frame: Option<model::FrameNumber>) -> Self {
        if limit_to_event && let Some(target_frame) = event_target_frame {
            Self::Frame(target_frame)
        } else {
            Self::None
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Patch<V> {
    pub origin: V,
    pub size: V,
}

#[derive(Debug, Clone)]
pub struct PatchResponse {
    pub data: Vec<f32>,
    pub patch: Patch<UVec2>,
}

pub struct Tracker<'a, V> {
    video: &'a V,
    patch_provider: fn(&V, model::FrameNumber, Patch<DVec2>) -> PatchResponse,
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
        patch_provider: fn(&V, model::FrameNumber, Patch<DVec2>) -> PatchResponse,
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

        let patch_request = Patch {
            origin: last_region.center - self.search_radius,
            size: DVec2::splat(2.0 * self.search_radius),
        };

        let patch_response_1 = (self.patch_provider)(self.video, self.last_frame, patch_request);
        let patch_response_2 = (self.patch_provider)(
            self.video,
            self.last_frame + model::FrameDelta(1),
            patch_request,
        );

        let image1 = mv::MonochromeImage::new(
            patch_response_1.data.as_slice(),
            patch_response_1.patch.size.try_into().unwrap(),
        );
        let image2 = mv::MonochromeImage::new(
            patch_response_2.data.as_slice(),
            patch_response_2.patch.size.try_into().unwrap(),
        );

        // In theory, the two different patch responses might have different origin points,
        // because the frames might be of a different size.
        let region1 = last_region.offset(-DVec2::from(patch_response_1.patch.origin));
        let predicted_region2 = last_region.offset(-DVec2::from(patch_response_2.patch.origin));

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
                self.track
                    .push(refined_region2.offset(DVec2::from(patch_response_2.patch.origin)));
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

        let initial_marker = Region::from_center_and_radius(DVec2 { x: 272.0, y: 81.0 }, 10.0);
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
