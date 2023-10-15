use std::borrow::Cow;
use std::fmt::{Debug, Display, Formatter};

use crate::subtitle::compile::{NdeError, NdeResult, NodeState};
use crate::{message, nde, style, subtitle, view};

#[derive(Clone)]
pub struct State {
    matrix: iced_node_editor::Matrix,
    filters: Vec<FilterReference>,
    selection_index: Option<usize>,
    selected_filter: Option<FilterReference>,
    pub dangling_source: Option<iced_node_editor::LogicalEndpoint>,
    pub dangling_connection: Option<iced_node_editor::Link>,
}

impl State {
    pub fn update_filter_names(&mut self, subtitles: &subtitle::SlineTrack) {
        self.filters.clear();
        for (i, filter) in subtitles.filters.iter().enumerate() {
            self.filters.push(FilterReference {
                name: filter.name.clone(),
                index: i,
            });
        }

        self.selection_index = None;
        self.selected_filter = None;
    }
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
            filters: vec![],
            selection_index: None,
            selected_filter: None,
            dangling_connection: None,
            dangling_source: None,
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FilterReference {
    pub name: String,
    pub index: usize,
}

impl Display for FilterReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.name.is_empty() {
            f.write_str("(unnamed filter)")
        } else {
            f.write_str(self.name.as_str())
        }
    }
}

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

#[derive(Debug, Clone, Copy)]
struct ConnectionStyle {
    colour: iced::Color,
}

impl iced_node_editor::styles::connection::StyleSheet for ConnectionStyle {
    type Style = iced::Theme;

    fn appearance(&self, _style: &Self::Style) -> iced_node_editor::styles::connection::Appearance {
        iced_node_editor::styles::connection::Appearance {
            color: Some(self.colour),
        }
    }
}

pub fn view<'a>(
    self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    pane_state: &'a State,
) -> super::View<'a> {
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
                    let nde_result_or_error = subtitle::compile::nde(
                        active_sline,
                        &nde_filter.graph,
                        global_state.frame_rate(),
                        &mut counter,
                    );

                    let mut graph_content = vec![];
                    let scale = pane_state.matrix.get_scale(); // For correct node grid translation behaviour

                    // Create `node_editor` nodes with sockets for each of the nodes in the filter,
                    // and append them to the content
                    create_nodes(&mut graph_content, nde_filter, &nde_result_or_error, scale);
                    create_connections(&mut graph_content, nde_filter, &nde_result_or_error);

                    // Append the dangling connection, if one exists
                    if let Some(link) = &pane_state.dangling_connection {
                        graph_content.push(iced_node_editor::Connection::new(link.clone()).into());
                    }

                    view_graph(
                        self_pane,
                        pane_state,
                        nde_filter,
                        &nde_result_or_error,
                        graph_content,
                    )
                }
                None => view_non_selected(self_pane, pane_state),
            }
        }
        None => iced::widget::text("No subtitle currently selected.").into(),
    };

    super::View {
        title: iced::widget::text("Node editor").into(),
        content: iced::widget::container(content)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_x()
            .center_y()
            .into(),
    }
}

fn create_nodes(
    graph_content: &mut Vec<iced_node_editor::GraphNodeElement<message::Message, iced::Renderer>>,
    nde_filter: &nde::Filter,
    nde_result_or_error: &Result<subtitle::compile::NdeResult, subtitle::compile::NdeError>,
    scale: f32,
) {
    // Convert NDE graph nodes into `iced_node_editor` nodes
    for (node_index, visual_node) in nde_filter.graph.nodes.iter().enumerate() {
        let node = &visual_node.node;

        // First, we need to create sockets for the node, based on the actual
        // values of intermediate type if present,
        // falling back on the desired/predicted types otherwise.
        let out_sockets: Cow<[nde::node::SocketType]> =
            create_out_sockets(nde_result_or_error, node_index, node.as_ref());
        let in_sockets: Cow<[nde::node::SocketType]> =
            create_in_sockets(nde_filter, nde_result_or_error, node_index, node.as_ref());

        // Iterate over the collected input and output types,
        // and create appropriately-styled sockets
        let mut node_sockets = vec![];
        for (role, sockets) in [
            (iced_node_editor::SocketRole::In, in_sockets),
            (iced_node_editor::SocketRole::Out, out_sockets),
        ] {
            for socket_type in &*sockets {
                // Call our own utility function to create the socket
                if let Some(new_socket) =
                    make_socket::<message::Message, iced::Renderer>(role, *socket_type)
                {
                    node_sockets.push(new_socket);
                }
            }
        }

        let node_border_colour = match &nde_result_or_error {
            Ok(nde_result) => match nde_result.intermediates.get(node_index) {
                Some(NodeState::Inactive) => style::SAMAKU_INACTIVE,
                Some(NodeState::Active(_)) => style::SAMAKU_PRIMARY,
                Some(NodeState::Error) => style::SAMAKU_DESTRUCTIVE,
                None => panic!("intermediate node not found"),
            },
            Err(_) => {
                // If there was an error, make all nodes appear red
                style::SAMAKU_DESTRUCTIVE
            }
        };

        let content_size = visual_node.node.content_size();

        graph_content.push(
            iced_node_editor::node(visual_node.node.content(node_index))
                .sockets(node_sockets)
                .padding(iced::Padding::from(12.0))
                .center_x()
                .center_y()
                .on_translate(move |(x, y)| {
                    message::Message::MoveNode(node_index, x / scale, y / scale)
                })
                .width(iced::Length::Fixed(content_size.width))
                .height(iced::Length::Fixed(content_size.height))
                .position(visual_node.position)
                .style(iced_node_editor::styles::node::Node::Custom(Box::new(
                    NodeStyle {
                        border_colour: node_border_colour,
                    },
                )))
                .into(),
        );
    }
}

fn create_out_sockets<'a>(
    nde_result_or_error: &Result<NdeResult, NdeError>,
    node_index: usize,
    node: &'a dyn nde::Node,
) -> Cow<'a, [nde::node::SocketType]> {
    // For the outputs, just iterate and merge one list
    // with the other. But first, we need to check the preconditions,
    // like whether the compilation was successful and whether the current node
    // is even active
    match &nde_result_or_error {
        Ok(nde_result) => match &nde_result.intermediates[node_index] {
            NodeState::Active(socket_values) => {
                let mut merged: Vec<nde::node::SocketType> = vec![];
                for (i, predicted) in node.predicted_outputs().iter().enumerate() {
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
    }
}

fn create_in_sockets<'a>(
    nde_filter: &nde::Filter,
    nde_result_or_error: &Result<NdeResult, NdeError>,
    node_index: usize,
    node: &'a dyn nde::Node,
) -> Cow<'a, [nde::node::SocketType]> {
    // For the inputs, we use the same general logic as before,
    // but instead of checking the node's sockets directly,
    // we need to check the nodes connecting into it
    match &nde_result_or_error {
        Ok(nde_result) => {
            // Note that here we don't check the active node's state,
            // because we can still make a judgment about its input types
            // even if it is inactive or errored.
            //
            // Initialise the result with the desired inputs,
            // overwriting later
            let mut merged: Vec<nde::node::SocketType> = Vec::from(node.desired_inputs());

            // Iterate over the nodes that connect into our node
            for (previous, next_socket_index) in nde_filter.graph.iter_previous(node_index) {
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
    }
}

fn create_connections(
    graph_content: &mut Vec<iced_node_editor::GraphNodeElement<message::Message, iced::Renderer>>,
    nde_filter: &nde::Filter,
    nde_result_or_error: &Result<NdeResult, NdeError>,
) {
    let connection_style = match nde_result_or_error {
        Ok(_) => ConnectionStyle {
            colour: style::SAMAKU_PRIMARY,
        },
        Err(_) => ConnectionStyle {
            colour: style::SAMAKU_DESTRUCTIVE,
        },
    };

    for (next, previous) in &nde_filter.graph.connections {
        graph_content.push(
            iced_node_editor::Connection::between(
                iced_node_editor::Endpoint::Socket(iced_node_editor::LogicalEndpoint {
                    node_index: previous.node_index,
                    role: iced_node_editor::SocketRole::Out,
                    socket_index: previous.socket_index,
                }),
                iced_node_editor::Endpoint::Socket(iced_node_editor::LogicalEndpoint {
                    node_index: next.node_index,
                    role: iced_node_editor::SocketRole::In,
                    socket_index: next.socket_index,
                }),
            )
            .style(iced_node_editor::styles::connection::Node::Custom(
                Box::new(connection_style),
            ))
            .into(),
        );
    }
}

fn view_graph<'a>(
    self_pane: super::Pane,
    pane_state: &State,
    nde_filter: &nde::Filter,
    nde_result_or_error: &Result<NdeResult, NdeError>,
    graph_content: std::vec::Vec<
        iced_node_editor::GraphNodeElement<'a, message::Message, iced::Renderer>,
    >,
) -> iced::Element<'a, message::Message> {
    let graph_container =
        iced_node_editor::graph_container::<message::Message, iced::Renderer>(graph_content)
            .dangling_source(pane_state.dangling_source)
            .on_translate(move |p| {
                message::Message::Pane(
                    self_pane,
                    message::Pane::NodeEditorTranslationChanged(p.0, p.1),
                )
            })
            .on_scale(move |x, y, s| {
                message::Message::Pane(self_pane, message::Pane::NodeEditorScaleChanged(x, y, s))
            })
            .on_connect(message::Message::ConnectNodes)
            .on_disconnect(move |endpoint, new_dangling_end_position| {
                message::Message::DisconnectNodes(endpoint, new_dangling_end_position, self_pane)
            })
            .on_dangling(move |maybe_dangling| {
                message::Message::Pane(self_pane, message::Pane::NodeEditorDangling(maybe_dangling))
            })
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .matrix(pane_state.matrix);

    let menu_bar = iced_aw::menu_bar!(add_menu())
        .item_width(iced_aw::menu::ItemWidth::Uniform(180))
        .item_height(iced_aw::menu::ItemHeight::Uniform(32));

    let unassign_button = iced::widget::button(iced::widget::text("Unassign"))
        .on_press(message::Message::UnassignFilterFromActiveSline);

    let name_box = iced::widget::text_input("Filter name", &nde_filter.name)
        .on_input(message::Message::SetActiveFilterName)
        .padding(5.0)
        .width(iced::Length::Fixed(200.0));

    let error_message = iced::widget::text(match nde_result_or_error {
        Ok(_) => "",
        Err(NdeError::CycleInGraph) => "Cycle detected!",
    })
    .style(style::SAMAKU_DESTRUCTIVE);

    let bottom_bar = iced::widget::container(
        iced::widget::row![
            menu_bar,
            unassign_button,
            name_box,
            iced::widget::horizontal_space(iced::Length::Fill),
            error_message
        ]
        .spacing(5.0)
        .align_items(iced::Alignment::Center),
    )
    .padding(5.0);

    let separator = iced_aw::quad::Quad {
        width: iced::Length::Fill,
        height: iced::Length::Fixed(0.5),
        color: style::samaku_theme()
            .extended_palette()
            .background
            .weak
            .color,
        inner_bounds: iced_aw::quad::InnerBounds::Ratio(1.0, 1.0),
        ..Default::default()
    };

    iced::widget::column![graph_container, separator, bottom_bar].into()
}

fn view_non_selected(
    self_pane: super::Pane,
    pane_state: &State,
) -> iced::Element<message::Message> {
    let selection_list = iced_aw::selection_list(
        pane_state.filters.as_slice(),
        move |selection_index, filter_ref| {
            message::Message::Pane(
                self_pane,
                message::Pane::NodeEditorFilterSelected(selection_index, filter_ref),
            )
        },
    )
    .width(iced::Length::Fixed(200.0))
    .height(iced::Length::Fixed(200.0));

    let assign_button = iced::widget::button(iced::widget::text("Assign")).on_press_maybe(
        pane_state
            .selected_filter
            .as_ref()
            .map(|filter_ref| message::Message::AssignFilterToActiveSline(filter_ref.index)),
    );
    let create_button = iced::widget::button(iced::widget::text("Create new"))
        .on_press(message::Message::CreateEmptyFilter);
    let delete_button = iced::widget::button(iced::widget::text("Delete")).on_press_maybe(
        pane_state
            .selected_filter
            .as_ref()
            .map(|filter_ref| message::Message::DeleteFilter(filter_ref.index)),
    );

    iced::widget::column![
        iced::widget::text("Filters").size(20),
        selection_list,
        iced::widget::row![assign_button, create_button, delete_button].spacing(5)
    ]
    .spacing(5)
    .into()
}

fn make_socket<'a, Message, Renderer>(
    role: iced_node_editor::SocketRole,
    socket_type: nde::node::SocketType,
) -> Option<iced_node_editor::Socket<'a, Message, Renderer>>
where
    Renderer: iced::advanced::text::Renderer + 'a,
    <Renderer as iced::advanced::Renderer>::Theme: iced::widget::text::StyleSheet,
{
    const BLOB_RADIUS: f32 = 7.0;

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

    // The style of the blob is not determined by a style sheet, but by properties of the `Socket`
    // itself.
    let (blob_border_radius, blob_color, label) = match socket_type {
        nde::node::SocketType::IndividualEvent => (0.0, iced::Color::from_rgb(1.0, 1.0, 1.0), ""),
        nde::node::SocketType::MultipleEvents => (0.0, crate::style::SAMAKU_PRIMARY, ""),
        nde::node::SocketType::AnyEvents => (0.0, crate::style::SAMAKU_BACKGROUND, ""),
        nde::node::SocketType::LocalTags => (
            BLOB_RADIUS,
            iced::Color::from_rgb(1.0, 1.0, 1.0),
            "Local tags",
        ),
        nde::node::SocketType::GlobalTags => (
            BLOB_RADIUS,
            iced::Color::from_rgb(0.5, 0.5, 0.5),
            "Global tags",
        ),
        nde::node::SocketType::Position => (
            BLOB_RADIUS,
            iced::Color::from_rgb(0.09, 0.81, 0.48),
            "Position",
        ),
        nde::node::SocketType::Rectangle => (
            BLOB_RADIUS,
            iced::Color::from_rgb(0.19, 0.90, 0.90),
            "Rectangle",
        ),
        nde::node::SocketType::FrameRate => (
            BLOB_RADIUS,
            iced::Color::from_rgb(0.73, 0.38, 0.76),
            "Frame rate",
        ),
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

fn menu_item(
    label: &str,
    node_shell: nde::node::Shell,
) -> iced_aw::menu::MenuTree<message::Message, iced::Renderer> {
    iced_aw::menu_tree!(
        view::menu::labeled_button(label, message::Message::AddNode(node_shell))
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
    )
}

fn sub_menu<'a>(
    label: &str,
    children: Vec<iced_aw::menu::MenuTree<'a, message::Message, iced::Renderer>>,
) -> iced_aw::menu::MenuTree<'a, message::Message, iced::Renderer> {
    view::menu::sub_menu(label, message::Message::None, children)
}

fn add_menu<'a>() -> iced_aw::menu::MenuTree<'a, message::Message, iced::Renderer> {
    iced_aw::helpers::menu_tree(
        iced::widget::button(iced::widget::text("Add node")).on_press(message::Message::None),
        vec![
            sub_menu(
                "Input",
                vec![
                    menu_item("Subtitle", nde::node::Shell::InputSline),
                    menu_item("Tags", nde::node::Shell::InputTags),
                    menu_item("Position", nde::node::Shell::InputPosition),
                    menu_item("Rectangle", nde::node::Shell::InputRectangle),
                    menu_item("Frame rate", nde::node::Shell::InputFrameRate),
                ],
            ),
            sub_menu(
                "Split",
                vec![menu_item(
                    "Frame by frame",
                    nde::node::Shell::SplitFrameByFrame,
                )],
            ),
            sub_menu(
                "Clip",
                vec![menu_item("Rectangular", nde::node::Shell::ClipRectangle)],
            ),
            menu_item("Italicise", nde::node::Shell::Italic),
            menu_item("Set position", nde::node::Shell::SetPosition),
            menu_item("Motion track", nde::node::Shell::MotionTrack),
            menu_item("Gradient", nde::node::Shell::Gradient),
        ],
    )
}

pub fn update(
    node_editor_state: &mut State,
    pane_message: message::Pane,
) -> iced::Command<message::Message> {
    match pane_message {
        message::Pane::NodeEditorScaleChanged(x, y, scale) => {
            node_editor_state.matrix = node_editor_state
                .matrix
                .translate(-x, -y)
                .scale(if scale > 0.0 { 1.2 } else { 1.0 / 1.2 })
                .translate(x, y);
        }
        message::Pane::NodeEditorTranslationChanged(x, y) => {
            node_editor_state.matrix = node_editor_state.matrix.translate(x, y);
        }
        message::Pane::NodeEditorDangling(Some((source, link))) => {
            node_editor_state.dangling_source = Some(source);
            node_editor_state.dangling_connection = Some(link);
        }
        message::Pane::NodeEditorDangling(None) => {
            node_editor_state.dangling_source = None;
            node_editor_state.dangling_connection = None;
        }
        message::Pane::NodeEditorFilterSelected(selection_index, filter_ref) => {
            node_editor_state.selection_index = Some(selection_index);
            node_editor_state.selected_filter = Some(filter_ref);
        }
        _ => (),
    }

    iced::Command::none()
}
