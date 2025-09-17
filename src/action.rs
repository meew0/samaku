//! Contains actions to be performed based on messages in `update`.
//!
//! The purpose of this module is to reduce code duplication for actions performed in multiple different places across the codebase, as well as reduce the code load within the `update` function.

use crate::media;
use std::path::PathBuf;

pub(crate) fn load_video(global_state: &crate::Samaku, path_buf: PathBuf) {
    global_state.workers.emit_load_video(path_buf);
}

pub(crate) fn load_audio(global_state: &crate::Samaku, path_buf: PathBuf) {
    let mut audio_lock = global_state.shared.audio.lock().unwrap();
    *audio_lock = Some(media::Audio::load(path_buf));
    drop(audio_lock);
    global_state.workers.emit_restart_audio();
}
