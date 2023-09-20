// Basic template for new panes, so I don't need to skeletonize one of the existing ones every time...
// The pane must also be registered in `PaneState`, and consequently in the dispatch methods.

use std::fmt::{Debug, Formatter};

use crate::message;

#[derive(Clone)]
pub struct State {
    pub(crate) matrix: iced_node_editor::Matrix,
}

// `iced_node_editor::Matrix` doesn't implement `Debug`.
// So we have to do this manually...
impl Debug for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("State { <opaque> }")
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            matrix: iced_node_editor::Matrix::identity(),
        }
    }
}

const NODE_WIDTH: f32 = 200.0;
const NODE_HEIGHT: f32 = 75.0;

pub fn view<'a>(
    self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    pane_state: &'a State,
) -> super::PaneView<'a> {
    let content: iced::Element<message::Message> = match global_state.active_sline_index {
        Some(active_sline_index) => {
            let active_sline = &global_state.subtitles.slines[active_sline_index];
            match active_sline.nde_filter_index {
                Some(nde_filter_index) => {
                    let nde_filter = &global_state.subtitles.filters[nde_filter_index];
                    let scale = pane_state.matrix.get_scale();
                    let mut graph_content = vec![];

                    for (i, visual_node) in nde_filter.graph.nodes.iter().enumerate() {
                        graph_content.push(
                            iced_node_editor::node(iced::widget::text(visual_node.node.name()))
                                .center_x()
                                .center_y()
                                .on_translate(move |(x, y)| {
                                    message::Message::MoveNode(i, x / scale, y / scale)
                                })
                                .width(iced::Length::Fixed(NODE_WIDTH))
                                .height(iced::Length::Fixed(NODE_HEIGHT))
                                .position(visual_node.position)
                                .into(),
                        );
                    }

                    for (next_endpoint, previous_endpoint) in nde_filter.graph.connections.iter() {
                        let from = &nde_filter.graph.nodes[previous_endpoint.node_index];
                        let to = &nde_filter.graph.nodes[next_endpoint.node_index];

                        graph_content.push(
                            iced_node_editor::connection(
                                iced::Point::new(
                                    from.position.x + NODE_WIDTH,
                                    from.position.y + (NODE_HEIGHT / 2.0),
                                ),
                                iced::Point::new(
                                    to.position.x,
                                    to.position.y + (NODE_HEIGHT / 2.0),
                                ),
                            )
                            .into(),
                        );
                    }

                    iced_node_editor::graph_container::<message::Message, iced::Renderer>(
                        graph_content,
                    )
                    .on_translate(move |p| {
                        message::Message::Pane(
                            self_pane,
                            message::PaneMessage::NodeEditorTranslationChanged(p.0, p.1),
                        )
                    })
                    .on_scale(move |x, y, s| {
                        message::Message::Pane(
                            self_pane,
                            message::PaneMessage::NodeEditorScaleChanged(x, y, s),
                        )
                    })
                    .width(iced::Length::Fill)
                    .height(iced::Length::Fill)
                    .matrix(pane_state.matrix)
                    .into()
                }
                None => {
                    iced::widget::text("Currently selected subtitle does not have an NDE filter.")
                        .into()
                }
            }
        }
        None => iced::widget::text("No subtitle currently selected.").into(),
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
    node_editor_state: &mut State,
    pane_message: message::PaneMessage,
) -> iced::Command<message::Message> {
    match pane_message {
        message::PaneMessage::NodeEditorScaleChanged(x, y, scale) => {
            node_editor_state.matrix = node_editor_state
                .matrix
                .translate(-x, -y)
                .scale(if scale > 0.0 { 1.2 } else { 1.0 / 1.2 })
                .translate(x, y);
        }
        message::PaneMessage::NodeEditorTranslationChanged(x, y) => {
            node_editor_state.matrix = node_editor_state.matrix.translate(x, y);
        }
        _ => (),
    }

    iced::Command::none()
}
