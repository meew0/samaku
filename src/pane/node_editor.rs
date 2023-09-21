use std::borrow::Cow;
use std::fmt::{Debug, Formatter};

use crate::subtitle::compile::NodeState;
use crate::{message, nde, style, subtitle};

#[derive(Clone)]
pub struct State {
    matrix: iced_node_editor::Matrix,
    pub dangling_source: Option<iced_node_editor::LogicalEndpoint>,
    pub dangling_connection: Option<iced_node_editor::Link>,
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
            dangling_connection: None,
            dangling_source: None,
        }
    }
}

const NODE_WIDTH: f32 = 200.0;
const NODE_HEIGHT: f32 = 75.0;

struct NodeStyle {
    border_colour: iced::Color,
}

impl iced_node_editor::styles::node::StyleSheet for NodeStyle {
    type Style = iced::Theme;

    fn appearance(&self, style: &Self::Style) -> iced_node_editor::styles::node::Appearance {
        let palette = style.extended_palette();

        iced_node_editor::styles::node::Appearance {
            background: Some(iced::Background::Color(palette.background.base.color)),
            border_color: self.border_colour,
            border_radius: 5.0,
            border_width: 1.0,
            text_color: Some(palette.primary.base.color),
        }
    }
}

pub fn view<'a>(
    self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    pane_state: &'a State,
) -> super::PaneView<'a> {
    let content: iced::Element<message::Message> = match global_state.active_sline_index {
        Some(active_sline_index) => {
            // There is an active sline, check whether it has an NDE filter
            let active_sline = &global_state.subtitles.slines[active_sline_index];
            match active_sline.nde_filter_index {
                Some(nde_filter_index) => {
                    // It has an NDE filter
                    let nde_filter = &global_state.subtitles.filters[nde_filter_index];

                    // Before doing much of anything else, we need to run the NDE filter —
                    // not to get the output events, but for the intermediate state,
                    // which lets us determine what style to draw nodes in, as well as provide
                    // precise information of what types sockets contain
                    let mut counter = 0;
                    let nde_result_or_error =
                        subtitle::compile::nde(active_sline, &nde_filter.graph, &mut counter);

                    let mut graph_content = vec![];
                    let scale = pane_state.matrix.get_scale(); // For correct node grid translation behaviour

                    // Convert NDE graph nodes into `iced_node_editor` nodes
                    for (node_index, visual_node) in nde_filter.graph.nodes.iter().enumerate() {
                        let node = &visual_node.node;

                        // First, we need to create sockets for the node, based on the actual
                        // values of intermediate type if present,
                        // falling back on the desired/predicted types otherwise.
                        //
                        // For the outputs, this is easy, just iterate and merge one list
                        // with the other. But first, we need to check the preconditions,
                        // like whether the compilation was successful and whether the current node
                        // is even active
                        let out_sockets: Cow<[nde::node::SocketType]> = match &nde_result_or_error {
                            Ok(nde_result) => match &nde_result.intermediates[node_index] {
                                NodeState::Active(socket_values) => {
                                    let mut merged: Vec<nde::node::SocketType> = vec![];
                                    for (i, predicted) in
                                        node.predicted_outputs().iter().enumerate()
                                    {
                                        match socket_values
                                            .get(i)
                                            .and_then(nde::node::SocketValue::as_type)
                                        {
                                            Some(actual_type) => {
                                                // We found the type the output socket actually has!
                                                merged.push(actual_type);
                                            }
                                            None => {
                                                // There is no more specific type, either because
                                                // there was no value at the index, or because the
                                                // value was `None` or another value that is not
                                                // representable as a `SocketType`
                                                merged.push(*predicted);
                                            }
                                        }
                                    }

                                    Cow::Owned(merged)
                                }
                                _ => {
                                    // If the node is inactive or errored, we have nothing else
                                    // to go by, so use the predicted outputs
                                    Cow::Borrowed(node.predicted_outputs())
                                }
                            },
                            Err(_) => {
                                // If there was a global error while running the filter,
                                // like for example a cycle in the graph, we have no further
                                // information to use, so just use the predicted outputs directly
                                Cow::Borrowed(node.predicted_outputs())
                            }
                        };

                        // For the inputs, we use the same general logic as before,
                        // but instead of checking the node's sockets directly,
                        // we need to check the nodes connecting into it
                        let in_sockets: Cow<[nde::node::SocketType]> = match &nde_result_or_error {
                            Ok(nde_result) => {
                                // Note that here we don't check the active node's state,
                                // because we can still make a judgment about its input types
                                // even if it is inactive or errored.
                                //
                                // Initialise the result with the desired inputs,
                                // overwriting later
                                let mut merged: Vec<nde::node::SocketType> =
                                    Vec::from(node.desired_inputs());

                                // Iterate over the nodes that connect into our node
                                for (previous, next_socket_index) in
                                    nde_filter.graph.iter_previous(node_index)
                                {
                                    // Check whether the previous node is active
                                    // (otherwise, ignore it)
                                    if let NodeState::Active(previous_socket_values) =
                                        &nde_result.intermediates[previous.node_index]
                                    {
                                        // Check whether the previous node has returned
                                        // a type-representable value at the given socket position
                                        if let Some(actual_type) = previous_socket_values
                                            .get(previous.socket_index)
                                            .and_then(nde::node::SocketValue::as_type)
                                        {
                                            merged[next_socket_index] = actual_type;
                                        }
                                    }
                                }

                                Cow::Owned(merged)
                            }
                            Err(_) => Cow::Borrowed(node.desired_inputs()),
                        };

                        // Iterate over the collected input and output types,
                        // and create appropriately-styled sockets
                        let mut node_sockets = vec![];
                        for (role, sockets) in [
                            (iced_node_editor::SocketRole::In, in_sockets),
                            (iced_node_editor::SocketRole::Out, out_sockets),
                        ] {
                            for socket_type in sockets.iter() {
                                // Call our own utility function to create the socket
                                if let Some(new_socket) =
                                    make_socket::<message::Message, iced::Renderer>(
                                        role,
                                        socket_type,
                                    )
                                {
                                    node_sockets.push(new_socket);
                                }
                            }
                        }

                        let node_border_colour = match &nde_result_or_error {
                            Ok(nde_result) => match nde_result.intermediates[node_index] {
                                NodeState::Inactive => style::SAMAKU_INACTIVE,
                                NodeState::Active(_) => style::SAMAKU_PRIMARY,
                                NodeState::Error => style::SAMAKU_DESTRUCTIVE,
                            },
                            Err(_) => {
                                // If there was an error, make all nodes appear red
                                style::SAMAKU_DESTRUCTIVE
                            }
                        };

                        graph_content.push(
                            iced_node_editor::node(iced::widget::text(visual_node.node.name()))
                                .sockets(node_sockets)
                                .padding(iced::Padding::from(12.0))
                                .center_x()
                                .center_y()
                                .on_translate(move |(x, y)| {
                                    message::Message::MoveNode(node_index, x / scale, y / scale)
                                })
                                .width(iced::Length::Fixed(NODE_WIDTH))
                                .height(iced::Length::Fixed(NODE_HEIGHT))
                                .position(visual_node.position)
                                .style(iced_node_editor::styles::node::Node::Custom(Box::new(
                                    NodeStyle {
                                        border_colour: node_border_colour,
                                    },
                                )))
                                .into(),
                        );
                    }

                    for (next, previous) in nde_filter.graph.connections.iter() {
                        graph_content.push(
                            iced_node_editor::Connection::between(
                                iced_node_editor::Endpoint::Socket(
                                    iced_node_editor::LogicalEndpoint {
                                        node_index: previous.node_index,
                                        role: iced_node_editor::SocketRole::Out,
                                        socket_index: previous.socket_index,
                                    },
                                ),
                                iced_node_editor::Endpoint::Socket(
                                    iced_node_editor::LogicalEndpoint {
                                        node_index: next.node_index,
                                        role: iced_node_editor::SocketRole::In,
                                        socket_index: next.socket_index,
                                    },
                                ),
                            )
                            .into(),
                        );
                    }

                    // Append the dangling connection, if one exists
                    if let Some(link) = &pane_state.dangling_connection {
                        graph_content.push(iced_node_editor::Connection::new(link.clone()).into())
                    }

                    iced_node_editor::graph_container::<message::Message, iced::Renderer>(
                        graph_content,
                    )
                    .dangling_source(pane_state.dangling_source)
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
                    .on_connect(message::Message::ConnectNodes)
                    .on_disconnect(move |endpoint, new_dangling_end_position| {
                        message::Message::DisconnectNodes(
                            endpoint,
                            new_dangling_end_position,
                            self_pane,
                        )
                    })
                    .on_dangling(move |maybe_dangling| {
                        message::Message::Pane(
                            self_pane,
                            message::PaneMessage::NodeEditorDangling(maybe_dangling),
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
        title: iced::widget::text("Node editor").into(),
        content: iced::widget::container(content)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_x()
            .center_y()
            .into(),
    }
}

fn make_socket<'a, Message, Renderer>(
    role: iced_node_editor::SocketRole,
    socket_type: &nde::node::SocketType,
) -> Option<iced_node_editor::Socket<'a, Message, Renderer>>
where
    Renderer: iced::advanced::text::Renderer + 'a,
    <Renderer as iced::advanced::Renderer>::Theme: iced::widget::text::StyleSheet,
{
    let (blob_side, content_alignment) = match role {
        iced_node_editor::SocketRole::In => (
            iced_node_editor::SocketSide::Left,
            iced::alignment::Horizontal::Left,
        ),
        iced_node_editor::SocketRole::Out => (
            iced_node_editor::SocketSide::Right,
            iced::alignment::Horizontal::Right,
        ),
    };

    const BLOB_RADIUS: f32 = 7.0;

    // The style of the blob is not determined by a style sheet, but by properties of the `Socket`
    // itself.
    let (blob_border_radius, blob_color, label) = match socket_type {
        nde::node::SocketType::IndividualEvent => (0.0, iced::Color::from_rgb(1.0, 1.0, 1.0), ""),
        nde::node::SocketType::MonotonicEvents => (0.0, crate::style::SAMAKU_PRIMARY, ""),
        nde::node::SocketType::GenericEvents => (0.0, crate::style::SAMAKU_BACKGROUND, ""),
        nde::node::SocketType::LeafInput(_) => return None,
    };

    Some(iced_node_editor::Socket {
        role,
        blob_side,
        content_alignment,

        blob_radius: BLOB_RADIUS,
        blob_border_radius,
        blob_color,
        content: iced::widget::text(label).into(), // Arbitrary widgets can be used here.

        min_height: 0.0,
        max_height: f32::INFINITY,
        blob_border_color: None, // If `None`, the one from the style sheet will be used.
    })
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
        message::PaneMessage::NodeEditorDangling(Some((source, link))) => {
            node_editor_state.dangling_source = Some(source);
            node_editor_state.dangling_connection = Some(link);
        }
        message::PaneMessage::NodeEditorDangling(None) => {
            node_editor_state.dangling_source = None;
            node_editor_state.dangling_connection = None;
        }
        _ => (),
    }

    iced::Command::none()
}
