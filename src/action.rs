//! Contains actions to be performed based on messages in `update`.
//!
//! The purpose of this module is to reduce code duplication for actions performed in multiple different places across the codebase, as well as reduce the code load within the `update` function.

use crate::update::{notify_filter_lists, notify_selected_events, notify_style_lists};
use crate::{media, subtitle};
use std::path::PathBuf;

pub(crate) fn replace_subtitle_file(
    global_state: &mut crate::Samaku,
    subtitle_file: subtitle::File,
) {
    global_state.subtitles = subtitle_file;
    global_state.selected_event_indices.clear();

    notify_selected_events(global_state);
    notify_filter_lists(global_state);
    notify_style_lists(global_state, true);
}

pub(crate) fn load_video(global_state: &crate::Samaku, path_buf: PathBuf) {
    global_state.workers.emit_load_video(path_buf);
}

pub(crate) fn load_audio(global_state: &crate::Samaku, path_buf: PathBuf) {
    let mut audio_lock = global_state.shared.audio.lock().unwrap();
    *audio_lock = Some(media::Audio::load(path_buf));
    drop(audio_lock);
    global_state.workers.emit_restart_audio();
}
