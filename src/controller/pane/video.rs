use crate::message::PaneMessage;

pub fn update(video_state: &mut crate::model::pane::video::State, pane_message: PaneMessage) {
    match pane_message {
        PaneMessage::VideoFrameAvailable(new_frame, handle) => {
            println!("frame {} available", new_frame);
            video_state.actual_frame = new_frame;
            video_state.image_handle = Some(handle);
        }
    }
}
