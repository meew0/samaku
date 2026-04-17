//! Contains actions to be performed based on messages in `update`.
//!
//! The purpose of this module is to reduce code duplication for actions performed in multiple different places across the codebase, as well as reduce the code load within the `update` function.

use crate::update::{notify_filter_lists, notify_selected_events, notify_style_lists};
use crate::{media, message, model, subtitle};
use std::mem::{replace, swap};
use std::path::PathBuf;
use std::sync::Arc;

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
    let body = path_buf.file_name().map_or_else(
        || "Unknown file".to_owned(),
        |file_name| format!("{}", file_name.display()),
    );
    let toast_id = global_state.toasts.progress("Indexing video", body);

    let progress_sender = global_state.workers.progress_sender();
    let progress_sender_done = global_state.workers.progress_sender();

    let indexer = media::Video::create_indexer(&path_buf);
    if let Some(mut indexer) = global_state.toasts.anyhow(indexer) {
        indexer.set_progress_callback(move |current, total| {
            #[expect(clippy::cast_precision_loss, reason = "unavoidable in this case")]
            let fraction = (current as f32) / (total as f32);
            progress_sender.update_progress(toast_id, fraction);
            // TODO: allow cancelling indexing
            model::CancellationState::Continue
        });

        // Make the indexer worker index the video and return a message that will then load the video after indexing is finished.
        // Also send a final progress=1.0 so the progress toast unfreezes and starts its closing countdown.
        global_state.workers.emit_index(indexer, move |index| {
            progress_sender_done.update_progress(toast_id, 1.0);
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

/// Represents an action where multiple events are edited at the same time.
///
/// Either only a `Single` event is set to a value,
/// multiple events are `All` set to the same value,
/// or `Individual`ly to the values given in the vec.
#[derive(Debug, Clone)]
pub enum MultiEdit<T: Clone> {
    Single(subtitle::EventIndex, T),
    All(Arc<[subtitle::EventIndex]>, T),
    Individual(Arc<[subtitle::EventIndex]>, Vec<T>),
}

impl<T: Clone> MultiEdit<T> {
    pub fn map<V: Clone, F: Fn(T) -> V>(self, map_fn: F) -> MultiEdit<V> {
        match self {
            MultiEdit::Single(index, value) => MultiEdit::Single(index, map_fn(value)),
            MultiEdit::All(indices, value) => MultiEdit::All(indices, map_fn(value)),
            MultiEdit::Individual(indices, values) => {
                MultiEdit::Individual(indices, values.into_iter().map(map_fn).collect())
            }
        }
    }

    /// Applies this multi-edit to the given event track.
    /// Takes a function that returns a mutable reference to the relevant field on the event.
    /// Then, this function will return a MultiEdit representing the old state (i.e. that will undo the edit).
    pub fn apply_accessor(
        self,
        event_track: &mut subtitle::EventTrack,
        accessor: for<'a> fn(&'a mut subtitle::Event<'static>) -> &'a mut T,
    ) -> MultiEdit<T> {
        match self {
            MultiEdit::Single(event_index, value) => {
                let old = replace(accessor(&mut event_track[event_index]), value);
                MultiEdit::Single(event_index, old)
            }
            MultiEdit::All(event_indices, value) => {
                let mut changed = Vec::with_capacity(event_indices.len());
                for event_index in event_indices.iter() {
                    let old = replace(accessor(&mut event_track[*event_index]), value.clone());
                    changed.push(old);
                }
                MultiEdit::Individual(event_indices, changed)
            }
            MultiEdit::Individual(event_indices, mut values) => {
                // we can reuse the `values` allocation
                for (i, value) in values.iter_mut().enumerate() {
                    let event_index = event_indices[i];
                    swap(accessor(&mut event_track[event_index]), value);
                }
                MultiEdit::Individual(event_indices, values)
            }
        }
    }

    /// Applies this multi-edit to the given event track.
    /// Takes a function that applies a single edit to an event. It should return the old value.
    /// Then, this function will return a MultiEdit representing the old state (i.e. that will undo the edit).
    pub fn apply_function(
        self,
        event_track: &mut subtitle::EventTrack,
        applier: fn(&mut subtitle::EventTrack, subtitle::EventIndex, T) -> T,
    ) -> MultiEdit<T> {
        match self {
            MultiEdit::Single(event_index, value) => {
                let old = applier(event_track, event_index, value);
                MultiEdit::Single(event_index, old)
            }
            MultiEdit::All(event_indices, value) => {
                let mut changed = Vec::with_capacity(event_indices.len());
                for event_index in event_indices.iter() {
                    let old = applier(event_track, *event_index, value.clone());
                    changed.push(old);
                }
                MultiEdit::Individual(event_indices, changed)
            }
            MultiEdit::Individual(event_indices, values) => {
                // in this case, we cannot trivially reuse the `values` allocation
                let mut changed = Vec::with_capacity(event_indices.len());
                for (i, value) in values.into_iter().enumerate() {
                    let event_index = event_indices[i];
                    let old = applier(event_track, event_index, value);
                    changed.push(old);
                }
                MultiEdit::Individual(event_indices, changed)
            }
        }
    }

    /// Asserts that this `MultiEdit` only contains a single value.
    /// Then, retrieves that value, and returns a new `MultiEdit` replacing it with a specified new value.
    pub fn unwrap_value<V: Clone>(self, new_value: V) -> (T, MultiEdit<V>) {
        match self {
            MultiEdit::Single(event_index, value) => {
                (value, MultiEdit::Single(event_index, new_value))
            }
            MultiEdit::All(event_indices, value) => {
                (value, MultiEdit::All(event_indices, new_value))
            }
            MultiEdit::Individual(_, _) => {
                panic!("Tried to call `unwrap_value` on `MultiEdit::Individual`")
            }
        }
    }
}
