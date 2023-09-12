use std::sync::Arc;

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
        GlobalMessage::NextFrame => {
            todo!();
        }
        GlobalMessage::PreviousFrame => {
            todo!();
        }
    }

    iced::Command::none()
}
