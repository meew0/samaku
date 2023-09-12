use crate::{
    controller, media,
    message::{self, GlobalMessage, Message},
    model,
};

pub fn global_update(
    global_state: &mut model::GlobalState,
    global_message: GlobalMessage,
) -> iced::Command<Message> {
    match global_message {
        GlobalMessage::SelectVideoFile => {
            return iced::Command::perform(
                rfd::AsyncFileDialog::new().pick_file(),
                Message::map_option(|handle: rfd::FileHandle| {
                    Message::Worker(message::WorkerMessage::VideoDecoder(
                        message::VideoDecoderMessage::LoadVideo(handle.path().to_path_buf()),
                    ))
                }),
            );
        }
        GlobalMessage::VideoLoaded(metadata) => {
            global_state.video_metadata = Some(*metadata);

            // Emit a playback step, such that the frame gets shown immediately
            return Message::command_all(message::playback_step_all());
        }
        GlobalMessage::SelectAudioFile => {
            return iced::Command::perform(
                rfd::AsyncFileDialog::new().pick_file(),
                Message::map_option(|handle: rfd::FileHandle| {
                    Message::Global(GlobalMessage::AudioFileSelected(
                        handle.path().to_path_buf(),
                    ))
                }),
            );
        }
        GlobalMessage::AudioFileSelected(path_buf) => {
            let mut audio_lock = global_state.audio.lock().unwrap();
            *audio_lock = Some(media::Audio::load(path_buf));

            return message::Message::command(message::Message::SpawnWorker(
                controller::workers::Type::CpalPlayback,
            ));
        }
        GlobalMessage::SelectSubtitleFile => {
            if let Some(_) = &global_state.video_metadata {
                let future = async {
                    match rfd::AsyncFileDialog::new().pick_file().await {
                        Some(handle) => {
                            Some(smol::fs::read_to_string(handle.path()).await.unwrap())
                        }
                        None => None,
                    }
                };
                return iced::Command::perform(
                    future,
                    Message::map_option(|content| {
                        Message::Global(GlobalMessage::SubtitleFileRead(content))
                    }),
                );
            }
        }
        GlobalMessage::SubtitleFileRead(content) => {
            if let Some(video_metadata) = &global_state.video_metadata {
                global_state.subtitles = Some(media::Subtitles::load_utf8(
                    content,
                    video_metadata.width,
                    video_metadata.height,
                ));
            }
        }
        GlobalMessage::PlaybackAdvanceFrames(delta_frames) => {
            if let Some(video_metadata) = &global_state.video_metadata {
                global_state
                    .playback_state
                    .add_frames(delta_frames, video_metadata.frame_rate);
            }
            return Message::command_all(message::playback_step_all());
        }
        GlobalMessage::PlaybackAdvanceSeconds(delta_seconds) => {
            global_state.playback_state.add_seconds(delta_seconds);
            return Message::command_all(message::playback_step_all());
        }
        GlobalMessage::TogglePlayback => {
            // For some reason `fetch_not`, which would perform a toggle in place,
            // is unstable. `fetch_xor` with true should be equivalent.
            global_state
                .playback_state
                .playing
                .fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    iced::Command::none()
}
