use std::sync::Arc;

use crate::media;

pub mod pane;
pub mod playback;

pub struct GlobalState {
    pub video: Option<media::Video>,
    pub subtitles: Option<media::Subtitles>,
    pub cpal_stream: Option<cpal::Stream>,
    pub playback_state: Arc<playback::PlaybackState>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            video: None,
            subtitles: None,
            cpal_stream: None,
            playback_state: Arc::new(playback::PlaybackState::default()),
        }
    }
}
