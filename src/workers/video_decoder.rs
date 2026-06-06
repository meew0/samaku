use crate::media::motion;
use crate::{media, message};
use std::collections::HashMap;
use std::{sync::Arc, thread};

#[derive(Debug)]
pub(super) enum MessageIn {
    PlaybackStep,
    LoadVideo(std::path::PathBuf, media::Index),
    TrackMotion(
        HashMap<motion::TrackId, motion::Marker>,
        media::FrameNumber,
        motion::Direction,
        motion::Target,
        motion::TrackSettings,
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
            let mut last_frame = media::FrameNumber(-1);

            let mut tracker_opt: Option<motion::Tracker<media::Video>> = None;

            loop {
                // Check if there's something to motion track. If it is, try to get a message to
                // see if there's something more important to do.
                let maybe_message = if let Some(ref mut tracker) = tracker_opt {
                    match rx_in.try_recv() {
                        Ok(message) => Some(message),
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // No message was received — motion tracking time!
                            let result = tracker.advance();
                            println!("{result:?}");

                            match result {
                                motion::TrackResult::Success => {
                                    tx_out.send(message::Message::MotionTrackUpdate(
                                        tracker.markers.clone(),
                                        tracker.current_frame,
                                    ));
                                }
                                motion::TrackResult::Failure | motion::TrackResult::Termination => {
                                    tracker_opt = None;
                                }
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
                        MessageIn::TrackMotion(
                            markers,
                            origin_frame,
                            direction,
                            target,
                            settings,
                        ) => {
                            if let Some(ref video) = video_opt {
                                tracker_opt = Some(motion::Tracker {
                                    video,
                                    patch_provider: media::Video::get_libmv_patch,
                                    current_frame: origin_frame,
                                    markers,
                                    direction,
                                    target,
                                    settings,
                                });
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
