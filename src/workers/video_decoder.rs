use std::{sync::Arc, thread};

use crate::{media, message};

#[derive(Debug, Clone)]
pub enum Message {
    PlaybackStep,
    LoadVideo(std::path::PathBuf),
    TrackMotionForNode(usize, media::motion::Region, i32, i32),
}

#[allow(clippy::too_many_lines)]
pub fn spawn(
    tx_out: super::GlobalSender,
    shared_state: &crate::SharedState,
) -> super::Worker<self::Message> {
    let (tx_in, rx_in) = std::sync::mpsc::channel::<self::Message>();

    let playback_position = Arc::clone(&shared_state.playback_position);

    let handle = thread::Builder::new()
        .name("samaku_video_decoder".to_string())
        .spawn(move || {
            let mut video_opt: Option<media::Video> = None;
            let mut last_frame: i32 = -1;

            let mut node_index = 0;
            let mut tracker_opt: Option<media::motion::Tracker<media::Video>> = None;

            loop {
                // Check if there's something to motion track. If it is, try to get a message to
                // see if there's something more important to do.
                let maybe_message = if let Some(ref mut tracker) = tracker_opt {
                    match rx_in.try_recv() {
                        Ok(message) => Some(message),
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // No message was received — motion tracking time!
                            let result = tracker.update(media::motion::Model::Translation);
                            println!("{result:?}");

                            match result {
                                media::motion::TrackResult::Success => {
                                    if tx_out
                                        .unbounded_send(message::Message::Node(
                                            node_index,
                                            message::Node::MotionTrackUpdate(
                                                tracker.last_tracked_frame(),
                                                *tracker.track().last().unwrap(),
                                            ),
                                        ))
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                                media::motion::TrackResult::Failure
                                | media::motion::TrackResult::Termination => tracker_opt = None,
                            }

                            None
                        }
                        Err(_) => return,
                    }
                } else {
                    // There's nothing to motion track, so wait for the next message
                    match rx_in.recv() {
                        Ok(message) => Some(message),
                        Err(_) => return,
                    }
                };

                // Process the received message, if it exists. If not, the loop will simply
                // continue.
                if let Some(message) = maybe_message {
                    match message {
                        self::Message::PlaybackStep => {
                            // The frame might have changed. Check whether we have a video
                            // and whether the frame has actually changed, and if it has,
                            // decode the new frame
                            if let Some(ref video) = video_opt {
                                let new_frame: i32 = playback_position
                                    .current_frame(video.metadata.frame_rate)
                                    .try_into()
                                    .expect("frame number overflow");
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
                            tracker_opt = None;
                            video_opt = Some(video);
                        }
                        self::Message::TrackMotionForNode(
                            new_node_index,
                            initial_region,
                            start_frame,
                            end_frame,
                        ) => {
                            if let Some(ref video) = video_opt {
                                node_index = new_node_index;
                                tracker_opt = Some(media::motion::Tracker::new(
                                    video,
                                    media::Video::get_libmv_patch,
                                    initial_region,
                                    60.0,
                                    start_frame,
                                    end_frame,
                                ));
                            }
                        }
                    }
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
