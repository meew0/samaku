use crate::message::PaneMessage;

pub fn update(video_state: &mut crate::model::pane::video::State, pane_message: PaneMessage) {
    match pane_message {
        PaneMessage::VideoIncrementCounter => {
            video_state.counter += 1;
        }
    }
}
