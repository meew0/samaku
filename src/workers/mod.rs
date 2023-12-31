use std::{cell::RefCell, thread};

use crate::{media, message, model};

mod cpal_playback;
mod log_toasts;
mod video_decoder;

#[derive(Debug, Clone)]
pub enum Type {
    VideoDecoder,
    CpalPlayback,
    LogToasts,
}

pub struct Worker<M> {
    worker_type: Type,
    _handle: thread::JoinHandle<()>,
    message_in: std::sync::mpsc::Sender<M>,
}

impl<M> Worker<M> {
    fn dispatch(&self, message: M) {
        self.message_in.send(message).unwrap_or_else(|err| {
            panic!(
                "Failed to send message to {:?} worker (error: {})",
                self.worker_type, err
            )
        });
    }
}

pub type GlobalReceiver = iced::futures::channel::mpsc::UnboundedReceiver<message::Message>;
pub type GlobalSender = iced::futures::channel::mpsc::UnboundedSender<message::Message>;

pub struct Workers {
    _sender: GlobalSender,
    pub receiver: RefCell<Option<GlobalReceiver>>,

    video_decoder: Worker<video_decoder::MessageIn>,
    cpal_playback: Worker<cpal_playback::MessageIn>,
    _log_toasts: Worker<log_toasts::MessageIn>,
}

impl Workers {
    /// Construct a new `Workers` instance with all workers spawned.
    #[must_use]
    pub fn spawn_all(shared_state: &crate::SharedState) -> Self {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();

        Self {
            video_decoder: video_decoder::spawn(sender.clone(), shared_state),
            cpal_playback: cpal_playback::spawn(sender.clone(), shared_state),
            _log_toasts: log_toasts::spawn(sender.clone(), shared_state),

            _sender: sender,
            receiver: RefCell::new(Some(receiver)),
        }
    }

    pub fn emit_play(&self) {
        self.cpal_playback.dispatch(cpal_playback::MessageIn::Play);
    }

    pub fn emit_pause(&self) {
        self.cpal_playback.dispatch(cpal_playback::MessageIn::Pause);
    }

    pub fn emit_playback_step(&self) {
        self.video_decoder
            .dispatch(video_decoder::MessageIn::PlaybackStep);
    }

    pub fn emit_load_video(&self, path_buf: std::path::PathBuf) {
        self.video_decoder
            .dispatch(video_decoder::MessageIn::LoadVideo(path_buf));
    }

    pub fn emit_restart_audio(&self) {
        self.cpal_playback
            .dispatch(cpal_playback::MessageIn::TryRestart);
    }

    pub fn emit_track_motion_for_node(
        &self,
        node_index: usize,
        initial_region: media::motion::Region,
        start_frame: model::FrameNumber,
        end_frame: model::FrameNumber,
    ) {
        self.video_decoder
            .dispatch(video_decoder::MessageIn::TrackMotionForNode(
                node_index,
                initial_region,
                start_frame,
                end_frame,
            ));
    }
}
