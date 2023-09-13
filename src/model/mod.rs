use std::sync::{Arc, Mutex};

use crate::media;

pub mod pane;
pub mod playback;

pub struct GlobalState {
    // The number of the frame that is actually being displayed right now,
    // together with the image it represents.
    // Will be slightly different from the information in
    // PlaybackState due to decoding latency etc.
    pub actual_frame: Option<(i32, iced::widget::image::Handle)>,

    pub video_metadata: Option<media::VideoMetadata>,
    pub subtitles: Option<media::Subtitles>,
    pub audio: Arc<Mutex<Option<media::Audio>>>,
    pub playback_state: Arc<playback::PlaybackState>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            actual_frame: None,
            video_metadata: None,
            subtitles: None,
            audio: Arc::new(Mutex::new(None)),
            playback_state: Arc::new(playback::PlaybackState::default()),
        }
    }
}
