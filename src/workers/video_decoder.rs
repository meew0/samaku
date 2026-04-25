use std::{sync::Arc, thread};

use crate::{media, message, model, nde, subtitle};

#[derive(Debug)]
pub(super) enum MessageIn {
    PlaybackStep,
    LoadVideo(std::path::PathBuf, media::Index),
    TrackMotionForNode(
        subtitle::ExtradataId,
        nde::graph::NodeId,
        media::motion::Region,
        model::FrameNumber,
        model::FrameNumber,
    ),
}

#[expect(
    clippy::too_many_lines,
    reason = "uncoupling all this code is kind of difficult and not so high priority"
)] // TODO uncouple
pub(super) fn spawn(
    tx_out: super::GlobalSender,
    shared_state: &crate::SharedState,
) -> super::Worker<MessageIn> {
    let (tx_in, rx_in) = std::sync::mpsc::channel::<MessageIn>();

    let playback_position = Arc::clone(&shared_state.playback_position);

    let handle = thread::Builder::new()
        .name("samaku_video_decoder".to_owned())
        .spawn(move || {
            let mut video_opt: Option<media::Video> = None;
            let mut last_frame = model::FrameNumber(-1);

            let mut node_index_opt: Option<(subtitle::ExtradataId, nde::graph::NodeId)> = None;
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
                                    let (filter_index, node_index) = node_index_opt.expect("motion tracking succeeded, but missing target node to send result to");
                                    tx_out.send(message::Message::Node(
                                        filter_index,
                                        node_index,
                                        message::Node::MotionTrackUpdate(
                                            tracker.last_tracked_frame(),
                                            *tracker.track().last().unwrap(),
                                        ),
                                    ));
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
                        MessageIn::PlaybackStep => {
                            // The frame might have changed. Check whether we have a video
                            // and whether the frame has actually changed, and if it has,
                            // decode the new frame
                            if let Some(ref video) = video_opt {
                                let new_frame =
                                    playback_position.current_frame(video.metadata.frame_rate);
                                if new_frame != last_frame {
                                    last_frame = new_frame;
                                    match video.get_iced_frame(new_frame) {
                                        Ok(handle) => {
                                            tx_out.send(message::Message::VideoFrameAvailable(
                                                new_frame, handle,
                                            ));
                                        }
                                        Err(err) => {
                                            tx_out.error(
                                                err,
                                                format!("Failed to decode frame {new_frame}"),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        MessageIn::LoadVideo(path_buf, index) => {
                            // Load new video
                            match media::Video::load(path_buf, index) {
                                Ok(video) => {
                                    let metadata_box = Box::new(video.metadata.clone());
                                    tx_out.send(message::Message::VideoLoaded(metadata_box));
                                    tracker_opt = None;
                                    video_opt = Some(video);
                                }
                                Err(err) => {
                                    // Display the error to the user as a toast
                                    tx_out.error(err, "Failed to load video");
                                }
                            }
                        }
                        MessageIn::TrackMotionForNode(
                        new_filter_index,
                            new_node_index,
                            initial_region,
                            start_frame,
                            end_frame,
                        ) => {
                            if let Some(ref video) = video_opt {
                                node_index_opt = Some((new_filter_index, new_node_index));
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
