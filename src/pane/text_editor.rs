use crate::message;

#[derive(Debug, Clone, Default)]
pub struct State {}

pub fn view<'a>(global_state: &'a crate::Samaku, _editor_state: &'a State) -> super::PaneView<'a> {
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
    _editor_state: &mut State,
    _pane_message: message::PaneMessage,
) -> iced::Command<message::Message> {
    iced::Command::none()
}
