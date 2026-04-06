//! Contains actions to be performed based on messages in `update`.
//!
//! The purpose of this module is to reduce code duplication for actions performed in multiple different places across the codebase, as well as reduce the code load within the `update` function.

use crate::update::{notify_filter_lists, notify_selected_events, notify_style_lists};
use crate::{media, message, model, subtitle};
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

pub(crate) fn index_video_and_load(global_state: &mut crate::Samaku, path_buf: PathBuf) {
    let toast_id = global_state.toasts.progress("Indexing video", "");

    let progress_sender = global_state.workers.progress_sender();

    let indexer = media::Video::create_indexer(&path_buf);
    if let Some(mut indexer) = global_state.toasts.anyhow(indexer) {
        indexer.set_progress_callback(move |current, total| {
            #[expect(clippy::cast_precision_loss, reason = "unavoidable in this case")]
            let fraction = (current as f32) / (total as f32);
            progress_sender.update_progress(toast_id, fraction);
            // TODO: allow cancelling indexing
            model::CancellationState::Continue
        });

        // Make the indexer worker index the video and return a message that will then load the video after indexing is finished
        global_state.workers.emit_index(indexer, move |index| {
            message::Message::VideoIndexed(path_buf, model::NeverClone(index))
        });
    }
}

pub(crate) fn load_video(global_state: &crate::Samaku, path_buf: PathBuf, index: media::Index) {
    global_state.workers.emit_load_video(path_buf, index);
}

pub(crate) fn load_audio(global_state: &mut crate::Samaku, path_buf: PathBuf) {
    let mut audio_lock = global_state.shared.audio.lock().unwrap();
    match media::Audio::load(path_buf) {
        Ok(audio) => {
            *audio_lock = Some(audio);
            drop(audio_lock);
        }
        Err(err) => {
            *audio_lock = None;
            drop(audio_lock);
            global_state.toasts.push(model::toast::Toast::message(
                model::toast::Status::Danger,
                "Error while loading audio file".to_owned(),
                format!("{err:?}"),
            ));
        }
    }
    global_state.workers.emit_restart_audio();
}
