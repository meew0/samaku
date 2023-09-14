use iced::advanced::Widget;

use crate::message;

#[derive(Debug, Clone)]
pub struct State {}

impl Default for State {
    fn default() -> Self {
        Self {}
    }
}

pub fn view<'a>(global_state: &'a crate::Samaku, pane_state: &'a State) -> super::PaneView<'a> {
    let content: iced::Element<message::Message> = match global_state.active_sline() {
        Some(active_sline) => iced::widget::responsive(|size| {
            iced::widget::text_input("Enter subtitle text...", &active_sline.text)
                .on_input(message::Message::SetActiveSlineText)
                .width(size.width)
                .line_height(iced::widget::text::LineHeight::Absolute(size.height.into()))
                .into()
        })
        .into(),
        None => iced::widget::text("No subtitle line currently selected.").into(),
    };

    super::PaneView {
        title: iced::widget::text("Pane title").into(),
        content: iced::widget::container(content)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_x()
            .center_y()
            .into(),
    }
}

pub fn update(
    grid_state: &mut State,
    pane_message: message::PaneMessage,
) -> iced::Command<message::Message> {
    match pane_message {
        _ => (),
    }

    return iced::Command::none();
}
