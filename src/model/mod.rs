use crate::media;

pub mod pane;

pub struct GlobalState {
    pub video: Option<media::Video>,
    pub subtitles: Option<media::Subtitles>,
    pub audio: Option<media::Audio>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            video: None,
            subtitles: None,
            audio: None,
        }
    }
}
