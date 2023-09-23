use std::{sync::Arc, thread};

use crate::{media, message};

#[derive(Debug, Clone)]
pub enum Message {
    PlaybackStep,
    LoadVideo(std::path::PathBuf),
}

pub fn spawn(
    tx_out: super::GlobalSender,
    shared_state: &crate::SharedState,
) -> super::Worker<self::Message> {
    let (tx_in, rx_in) = std::sync::mpsc::channel::<self::Message>();

    let playback_state = Arc::clone(&shared_state.playback_state);

    let handle = thread::Builder::new()
        .name("samaku_video_decoder".to_string())
        .spawn(move || {
            let mut video_opt: Option<media::Video> = None;
            let mut last_frame: i32 = -1;
            loop {
                match rx_in.recv() {
                    Ok(message) => match message {
                        self::Message::PlaybackStep => {
                            // The frame might have changed. Check whether we have a video
                            // and whether the frame has actually changed, and if it has,
                            // decode the new frame
                            if let Some(ref video) = video_opt {
                                let new_frame =
                                    playback_state.current_frame(video.metadata.frame_rate);
                                if new_frame != last_frame {
                                    last_frame = new_frame;
                                    let handle = video.get_iced_frame(new_frame);
                                    if tx_out
                                        .unbounded_send(message::Message::VideoFrameAvailable(
                                            new_frame, handle,
                                        ))
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                            }
                        }
                        self::Message::LoadVideo(path_buf) => {
                            // Load new video
                            let video = media::Video::load(path_buf);
                            let metadata_box = Box::new(video.metadata);
                            if tx_out
                                .unbounded_send(message::Message::VideoLoaded(metadata_box))
                                .is_err()
                            {
                                return;
                            }
                            video_opt = Some(video);
                        }
                    },
                    Err(_) => return,
                }
            }
        })
        .unwrap();

    super::Worker {
        worker_type: super::Type::VideoDecoder,
        _handle: handle,
        message_in: tx_in,
    }
}
