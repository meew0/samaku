use crate::message;

#[derive(Debug, Clone, Default)]
pub struct State {}

pub fn view<'a>(
    _self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    _editor_state: &'a State,
) -> super::View<'a> {
    let content: iced::Element<message::Message> = match global_state
        .subtitles
        .events
        .active_event(global_state.active_event_index)
    {
        Some(active_event) => iced::widget::responsive(|size| {
            iced::widget::text_input("Enter subtitle text...", &active_event.text)
                .on_input(message::Message::SetActiveEventText)
                .width(size.width)
                .line_height(iced::widget::text::LineHeight::Absolute(size.height.into()))
                .into()
        })
        .into(),
        None => iced::widget::text("No subtitle line currently selected.").into(),
    };

    super::View {
        title: iced::widget::text("Text editor").into(),
        content: iced::widget::container(content)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_x()
            .center_y()
            .into(),
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn update(
    _editor_state: &mut State,
    _pane_message: message::Pane,
) -> iced::Command<message::Message> {
    iced::Command::none()
}
