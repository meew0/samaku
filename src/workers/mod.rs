use crate::media::motion;
use crate::{media, message, model};
use std::collections::HashMap;
use std::{cell::RefCell, thread};

mod cpal_playback;
mod indexer;
mod log_toasts;
mod progress_listener;
mod video_decoder;

#[derive(Debug, Clone)]
pub enum Type {
    VideoDecoder,
    Indexer,
    CpalPlayback,
    LogToasts,
    ProgressListener,
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

#[derive(Debug, Clone)]
pub struct GlobalSender(iced::futures::channel::mpsc::UnboundedSender<message::Message>);

impl GlobalSender {
    /// Send a message to be processed by iced.
    ///
    /// # Panics
    /// Panics if the channel has been closed.
    pub fn send(&self, message: message::Message) {
        self.0
            .unbounded_send(message)
            .expect("Failed to send message from worker");
    }

    #[expect(clippy::needless_pass_by_value, reason = "for more ergonomic usage")]
    pub fn error<S: Into<String>>(&self, err: anyhow::Error, info: S) {
        self.send(message::toast_danger(info.into(), format!("{err:#}")));
    }
}

pub struct Workers {
    _sender: GlobalSender,
    pub receiver: RefCell<Option<GlobalReceiver>>,

    video_decoder: Worker<video_decoder::MessageIn>,
    indexer: Worker<indexer::MessageIn>,
    cpal_playback: Worker<cpal_playback::MessageIn>,
    _log_toasts: Worker<log_toasts::MessageIn>,
    progress_listener: Worker<progress_listener::MessageIn>,
}

impl Workers {
    /// Construct a new `Workers` instance with all workers spawned.
    #[must_use]
    pub fn spawn_all(shared_state: &crate::SharedState) -> Self {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
        let global_sender = GlobalSender(sender);

        Self {
            video_decoder: video_decoder::spawn(global_sender.clone(), shared_state),
            indexer: indexer::spawn(global_sender.clone(), shared_state),
            cpal_playback: cpal_playback::spawn(global_sender.clone(), shared_state),
            _log_toasts: log_toasts::spawn(global_sender.clone(), shared_state),
            progress_listener: progress_listener::spawn(global_sender.clone(), shared_state),

            _sender: global_sender,
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

    pub fn emit_index<F: FnOnce(media::Index) -> message::Message + Send + 'static>(
        &self,
        indexer: media::Indexer,
        message_callback: F,
    ) {
        self.indexer.dispatch(indexer::MessageIn::Index(
            indexer,
            Box::new(message_callback),
        ));
    }

    pub fn emit_load_video(&self, path_buf: std::path::PathBuf, index: media::Index) {
        self.video_decoder
            .dispatch(video_decoder::MessageIn::LoadVideo(path_buf, index));
    }

    pub fn emit_restart_audio(&self) {
        self.cpal_playback
            .dispatch(cpal_playback::MessageIn::TryRestart);
    }

    pub fn emit_track_motion(
        &self,
        markers: HashMap<motion::TrackId, motion::Marker>,
        origin_frame: media::FrameNumber,
        direction: motion::Direction,
        target: motion::Target,
        settings: motion::TrackSettings,
    ) {
        self.video_decoder
            .dispatch(video_decoder::MessageIn::TrackMotion(
                markers,
                origin_frame,
                direction,
                target,
                settings,
            ));
    }

    /// Returns a clone of the `ProgressListener` worker input channel,
    /// such that other threads etc. can asynchronously send progress updates
    /// to be dealt with appropriately inside the worker.
    pub fn progress_sender(&self) -> ProgressSender {
        ProgressSender(self.progress_listener.message_in.clone())
    }
}

pub struct ProgressSender(std::sync::mpsc::Sender<progress_listener::MessageIn>);

impl ProgressSender {
    /// Send a progress update to the progress worker.
    ///
    /// # Panics
    /// Panics if sending the message over the underlying channel fails for any reason.
    pub fn update_progress(&self, id: model::toast::Id, progress: f32) {
        self.0
            .send(progress_listener::MessageIn::Progress(id, progress))
            .expect("failed to send progress update");
    }
}
