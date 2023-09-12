use std::{sync::Arc, thread};

use crate::{media, message, model};

pub fn spawn(
    tx_out: super::GlobalSender,
    global_state: &model::GlobalState,
) -> Option<super::Worker<thread::JoinHandle<()>, message::VideoDecoderMessage>> {
    let playback_state = Arc::clone(&global_state.playback_state);
    let (tx_in, rx_in) = std::sync::mpsc::channel::<message::VideoDecoderMessage>();

    let handle = thread::spawn(move || {
        let mut video_opt: Option<media::Video> = None;
        let mut last_frame: i32 = -1;
        loop {
            match rx_in.recv() {
                Ok(message) => match message {
                    message::VideoDecoderMessage::PlaybackStep => {
                        // The frame might have changed. Check whether we have a video
                        // and whether the frame has actually changed, and if it has,
                        // decode the new frame
                        if let Some(ref video) = video_opt {
                            let new_frame = playback_state.current_frame(video.metadata.frame_rate);
                            if new_frame != last_frame {
                                last_frame = new_frame;
                                let handle = video.get_frame(new_frame);
                                if let Err(_) = tx_out.unbounded_send(message::Message::Pane(
                                    message::PaneMessage::VideoFrameAvailable(new_frame, handle),
                                )) {
                                    return;
                                }
                            }
                        }
                    }
                    message::VideoDecoderMessage::LoadVideo(path_buf) => {
                        // Load new video
                        let video = media::Video::load(path_buf);
                        let metadata_box = Box::new(video.metadata);
                        if let Err(_) = tx_out.unbounded_send(message::Message::Global(
                            message::GlobalMessage::VideoLoaded(metadata_box),
                        )) {
                            return;
                        }
                        video_opt = Some(video);
                    }
                },
                Err(_) => return,
            }
        }
    });

    let worker = super::Worker {
        worker_type: super::Type::VideoDecoder,
        handle,
        message_in: tx_in,
    };

    Some(worker)
}
