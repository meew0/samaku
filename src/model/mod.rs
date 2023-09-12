use std::sync::{Arc, Mutex};

use crate::media;

pub mod pane;
pub mod playback;

pub struct GlobalState {
    pub video_metadata: Option<media::VideoMetadata>,
    pub subtitles: Option<media::Subtitles>,
    pub audio: Arc<Mutex<Option<media::Audio>>>,
    pub playback_state: Arc<playback::PlaybackState>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            video_metadata: None,
            subtitles: None,
            audio: Arc::new(Mutex::new(None)),
            playback_state: Arc::new(playback::PlaybackState::default()),
        }
    }
}
