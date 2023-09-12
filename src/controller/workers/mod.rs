mod cpal_playback;
mod video_decoder;

use std::{cell::RefCell, thread};

use crate::{message, model};

#[derive(Debug, Clone, Copy, Hash)]
pub enum Type {
    VideoDecoder,
    CpalPlayback,
}

pub struct Worker<H, M> {
    worker_type: Type,
    _handle: H,
    message_in: std::sync::mpsc::Sender<M>,
}

pub type GlobalReceiver = iced::futures::channel::mpsc::UnboundedReceiver<message::Message>;
pub type GlobalSender = iced::futures::channel::mpsc::UnboundedSender<message::Message>;

pub struct Workers {
    pub receiver: RefCell<Option<GlobalReceiver>>,
    sender: GlobalSender,
    video_decoder: Option<Worker<thread::JoinHandle<()>, message::VideoDecoderMessage>>,
    cpal_playback: Option<Worker<cpal::Stream, ()>>,
}

fn try_dispatch<H, M>(worker_opt: &Option<Worker<H, M>>, message: M) {
    if let Some(worker) = worker_opt {
        // Can fail if the channel is closed.
        // For now, just ignore.
        // TODO: possibly drop the worker or something in this case
        let _ = worker.message_in.send(message);
    }
}

fn try_spawn<H, M, F: FnOnce(GlobalSender, &model::GlobalState) -> Option<Worker<H, M>>>(
    worker_opt: &mut Option<Worker<H, M>>,
    spawn_func: F,
    sender: GlobalSender,
    global_state: &model::GlobalState,
) {
    if let Some(_) = worker_opt {
        return;
    }

    *worker_opt = spawn_func(sender, global_state);
}

impl Workers {
    pub fn dispatch_update(&self, message: message::WorkerMessage) {
        match message {
            message::WorkerMessage::VideoDecoder(inner) => try_dispatch(&self.video_decoder, inner),
        }
    }

    pub fn spawn(&mut self, worker_type: Type, global_state: &model::GlobalState) {
        let sender = self.sender.clone();

        match worker_type {
            Type::VideoDecoder => try_spawn(
                &mut self.video_decoder,
                video_decoder::spawn,
                sender,
                global_state,
            ),
            Type::CpalPlayback => try_spawn(
                &mut self.cpal_playback,
                cpal_playback::spawn,
                sender,
                global_state,
            ),
        }
    }
}

impl Default for Workers {
    fn default() -> Self {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();

        Self {
            sender,
            receiver: RefCell::new(Some(receiver)),
            video_decoder: None,
            cpal_playback: None,
        }
    }
}
