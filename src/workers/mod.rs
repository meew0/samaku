mod cpal_playback;
mod video_decoder;

use std::{cell::RefCell, thread};

use crate::message;

#[derive(Debug, Clone)]
pub enum Type {
    VideoDecoder,
    CpalPlayback,
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
    sender: GlobalSender,
    pub receiver: RefCell<Option<GlobalReceiver>>,

    video_decoder: Worker<video_decoder::Message>,
    cpal_playback: Worker<cpal_playback::Message>,
}

impl Workers {
    /// Construct a new `Workers` instance with all workers spawned.
    pub fn spawn_all(shared_state: &crate::SharedState) -> Self {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();

        Self {
            video_decoder: video_decoder::spawn(sender.clone(), shared_state),
            cpal_playback: cpal_playback::spawn(sender.clone(), shared_state),

            sender: sender,
            receiver: RefCell::new(Some(receiver)),
        }
    }

    pub fn emit_playback_step(&self) {
        self.video_decoder
            .dispatch(video_decoder::Message::PlaybackStep);
    }

    pub fn emit_load_video(&self, path_buf: std::path::PathBuf) {
        self.video_decoder
            .dispatch(video_decoder::Message::LoadVideo(path_buf));
    }

    pub fn emit_restart_audio(&self) {
        self.cpal_playback
            .dispatch(cpal_playback::Message::TryRestart);
    }
}
