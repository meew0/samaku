use crate::{
    media,
    message::{GlobalMessage, Message},
    model,
};

pub fn global_update(
    global_state: &mut model::GlobalState,
    global_message: GlobalMessage,
) -> iced::Command<Message> {
    match global_message {
        GlobalMessage::LoadVideo => {
            return iced::Command::perform(
                rfd::AsyncFileDialog::new().pick_file(),
                Message::map_option(|handle: rfd::FileHandle| {
                    Message::Global(GlobalMessage::VideoFileSelected(
                        handle.path().to_path_buf(),
                    ))
                }),
            );
        }
        GlobalMessage::VideoFileSelected(path_buf) => {
            global_state.video = Some(media::Video::load(path_buf));
        }
        GlobalMessage::LoadAudio => {
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
            global_state.audio = Some(media::Audio::load(path_buf));
        }
        GlobalMessage::LoadSubtitles => {
            if let Some(_) = &global_state.video {
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
            if let Some(video) = &global_state.video {
                global_state.subtitles = Some(media::Subtitles::load_utf8(
                    content,
                    video.width,
                    video.height,
                ));
            }
        }
        GlobalMessage::NextFrame => {
            global_state.video.as_mut().map(|video| video.next_frame());
        }
        GlobalMessage::PreviousFrame => {
            global_state
                .video
                .as_mut()
                .map(|video| video.previous_frame());
        }
    }

    iced::Command::none()
}
