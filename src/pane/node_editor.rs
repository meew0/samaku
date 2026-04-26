use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{Debug, Display, Formatter};
use std::sync::LazyLock;

use crate::nde::graph::{NodeId, SocketId};
use crate::subtitle::compile::{NdeError, NdeResult, NodeState};
use crate::{message, model, nde, style, subtitle, view};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct State {
    camera: Camera,
    filters: Vec<FilterReference>,
    selection_index: Option<usize>,
    selected_filter: Option<FilterReference>,
    selected_nodes: Vec<NodeId>,
}

#[typetag::serde(name = "node_editor")]
impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        let content: iced::Element<message::Message> =
            if let Some(active_event_index) = global_state.selected_events.active() {
                // Exactly one event selected
                let active_event = &global_state.subtitles.events[active_event_index];

                // Check whether the event has an NDE filter assigned. If yes, display the node editor
                // to edit that filter, otherwise, display the assignment pane
                match global_state
                    .subtitles
                    .extradata
                    .nde_filter_and_id_for_event(active_event)
                {
                    Some((nde_filter_id, nde_filter)) => view_filter(
                        self_pane,
                        global_state,
                        self,
                        active_event,
                        nde_filter_id,
                        nde_filter,
                    ),
                    None => view_non_selected(self_pane, self, false),
                }
            } else if global_state.selected_events.is_empty() {
                iced::widget::text("No subtitle currently selected.").into()
            } else {
                // Multiple events selected. We can't meaningfully run the filter on multiple events
                // at once, even if their filters should match, so display the assignment pane
                // as a fallback so at least a filter can be assigned to multiple events
                view_non_selected(self_pane, self, true)
            };

        super::View {
            title: iced::widget::text("Node editor").into(),
            content: iced::widget::container(content)
                .center_x(iced::Length::Fill)
                .center_y(iced::Length::Fill)
                .into(),
        }
    }

    fn update(&mut self, pane_message: message::Pane) -> iced::Task<message::Message> {
        match pane_message {
            message::Pane::NodeEditorCameraChanged(point, zoom) => {
                self.camera = Camera::new(point, zoom);
            }
            message::Pane::NodeEditorSelectionChanged(selected) => {
                self.selected_nodes = selected;
            }
            message::Pane::NodeEditorFilterSelected(selection_index, filter_ref) => {
                self.selection_index = Some(selection_index);
                self.selected_filter = Some(filter_ref);
            }
            _ => (),
        }

        iced::Task::none()
    }

    fn visit(&mut self, visitor: &mut dyn super::Visitor) {
        visitor.visit_node_editor(self);
    }

    fn update_selected_events(
        &mut self,
        _selected_events: &model::select::EventSelection,
        _events: &subtitle::EventTrack,
    ) {
        // Always unset the node selection if the event selection changed, since we can't easily tell
        // whether the displayed filter is the same.
        // This does not currently affect the selection that is visible on `iced_nodegraph`,
        // since setting that is buggy (see `create_graph`)
        // TODO: if this ends up being annoying, we might have to check that anyway.
        self.selected_nodes.clear();
    }

    fn update_filter_names(&mut self, extradata: &subtitle::Extradata) {
        self.filters.clear();
        for (i, filter) in extradata.iter_filters() {
            self.filters.push(FilterReference {
                name: filter.name.clone(),
                index: i,
            });
        }

        self.selection_index = None;
        self.selected_filter = None;
    }
}

impl State {
    pub fn clear_selected(&mut self) {
        self.selected_nodes.clear();
    }

    pub fn remap_selected(&mut self, mapping: &[Option<NodeId>]) {
        self.selected_nodes.retain_mut(|id| {
            if let Some(new_id) = mapping[id.0] {
                *id = new_id;
                true
            } else {
                false
            }
        });
    }
}

inventory::submit! {
    super::Shell::new(
        "Node editor",
        || Box::new(State::default())
    )
}

// `iced_node_editor::Matrix` doesn't implement `Debug`.
// So we have to do this manually...
impl Debug for State {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("State { <opaque> }")
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            camera: Camera::new(iced::Point::ORIGIN, 1.0),
            filters: vec![],
            selection_index: None,
            selected_filter: None,
            selected_nodes: vec![],
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Camera {
    position_x: f32,
    position_y: f32,
    zoom: f32,
}

impl Camera {
    fn new(position: iced::Point, zoom: f32) -> Self {
        Self {
            position_x: position.x,
            position_y: position.y,
            zoom,
        }
    }

    fn position(&self) -> iced::Point {
        iced::Point::new(self.position_x, self.position_y)
    }
}

impl iced_nodegraph::NodeId for NodeId {}

/// Pin IDs and socket IDs need to be different,
/// since the NDE code assumes socket IDs are only unique per side,
/// whereas `iced_nodegraph` assumes pin IDs are unique per node.
/// So we do this in a pragmatic way by using negative numbers (shifted by -1) for inputs,
/// and positive numbers for outputs.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
struct PinId(isize);

impl PinId {
    fn input(socket_id: SocketId) -> Self {
        Self(-1_isize - socket_id.0.cast_signed())
    }

    fn output(socket_id: SocketId) -> Self {
        Self(socket_id.0.cast_signed())
    }

    fn socket_id(self) -> SocketId {
        if self.0 < 0 {
            SocketId((-1 - self.0).cast_unsigned())
        } else {
            SocketId(self.0.cast_unsigned())
        }
    }
}

impl iced_nodegraph::PinId for PinId {}

struct SocketRole {
    side: iced_nodegraph::PinSide,
    direction: iced_nodegraph::PinDirection,
    pin_id_func: fn(SocketId) -> PinId,
}

impl SocketRole {
    const IN: SocketRole = SocketRole {
        side: iced_nodegraph::PinSide::Left,
        direction: iced_nodegraph::PinDirection::Input,
        pin_id_func: PinId::input,
    };
    const OUT: SocketRole = SocketRole {
        side: iced_nodegraph::PinSide::Right,
        direction: iced_nodegraph::PinDirection::Output,
        pin_id_func: PinId::output,
    };
}

type NodeGraph<'a> = iced_nodegraph::NodeGraph<
    'a,
    NodeId,
    PinId,
    usize,
    message::Message,
    iced::Theme,
    iced::Renderer,
>;

#[derive(Debug, Clone, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FilterReference {
    pub name: String,
    pub index: subtitle::ExtradataId,
}

impl Display for FilterReference {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        if self.name.is_empty() {
            formatter.write_str("(unnamed filter)")
        } else {
            formatter.write_str(self.name.as_str())
        }
    }
}

fn view_filter<'a>(
    self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    pane_state: &'a State,
    active_event: &subtitle::Event<'static>,
    nde_filter_id: subtitle::ExtradataId,
    nde_filter: &nde::Filter,
) -> iced::Element<'a, message::Message> {
    // Before doing much of anything else, we need to run the NDE filter —
    // not to get the output events, but for the intermediate state,
    // which lets us determine what style to draw nodes in, as well as provide
    // precise information of what types sockets contain
    let context = global_state.compile_context();
    let nde_result_or_error = subtitle::compile::nde(active_event, &nde_filter.graph, &context);

    // Create the (empty) node graph
    let mut graph = create_graph(self_pane, pane_state, nde_filter_id);

    // Create `node_editor` nodes with sockets for each of the nodes in the filter,
    // and append them to the content
    create_nodes(&mut graph, nde_filter_id, nde_filter, &nde_result_or_error);
    create_connections(&mut graph, nde_filter, &nde_result_or_error);

    view_graph(
        pane_state,
        nde_filter_id,
        nde_filter,
        &nde_result_or_error,
        graph,
    )
}

fn create_graph(
    self_pane: super::Pane,
    pane_state: &'_ State,
    nde_filter_id: subtitle::ExtradataId,
) -> Box<NodeGraph<'_>> {
    let mut graph: NodeGraph = iced_nodegraph::NodeGraph::default();

    graph = graph
        .on_connect(move |previous, next| {
            message::Message::ConnectNodes(
                nde_filter_id,
                nde::graph::PreviousEndpoint {
                    node_index: previous.node_id,
                    socket_index: previous.pin_id.socket_id(),
                },
                nde::graph::NextEndpoint {
                    node_index: next.node_id,
                    socket_index: next.pin_id.socket_id(),
                },
            )
        })
        .on_disconnect(move |previous, next| {
            message::Message::DisconnectNodes(
                nde_filter_id,
                nde::graph::PreviousEndpoint {
                    node_index: previous.node_id,
                    socket_index: previous.pin_id.socket_id(),
                },
                nde::graph::NextEndpoint {
                    node_index: next.node_id,
                    socket_index: next.pin_id.socket_id(),
                },
            )
        })
        .on_move(move |node_id, point| message::Message::MoveNode(nde_filter_id, node_id, point))
        .on_select(move |nodes| {
            message::Message::Batch(vec![
                message::Message::Pane(
                    self_pane,
                    message::Pane::NodeEditorSelectionChanged(nodes.clone()),
                ),
                message::Message::ActivateNodes(nde_filter_id, nodes),
            ])
        })
        .on_group_move(move |node_ids, vector| {
            message::Message::MoveNodeGroup(nde_filter_id, node_ids, vector)
        })
        .on_camera_change(move |position, zoom| {
            message::Message::Pane(
                self_pane,
                message::Pane::NodeEditorCameraChanged(position, zoom),
            )
        })
        .on_delete(move |node_ids| message::Message::DeleteNodes(nde_filter_id, node_ids))
        //.selection(dbg!(&pane_state.selected_nodes)) // TODO: this method is buggy (if this line is left in, no nodes can be selected at all)
        .initial_camera(pane_state.camera.position(), pane_state.camera.zoom)
        .width(iced::Length::Fill)
        .height(iced::Length::Fill);

    Box::new(graph)
}

fn create_nodes(
    graph: &mut NodeGraph,
    nde_filter_id: subtitle::ExtradataId,
    nde_filter: &nde::Filter,
    nde_result_or_error: &Result<NdeResult, NdeError>,
) {
    // Convert NDE graph nodes into `iced_node_editor` nodes
    for (node_index, visual_node) in nde_filter.graph.nodes.iter().enumerate() {
        let node = &visual_node.node;
        let node_id = NodeId(node_index);

        // First, we need to create sockets for the node, based on the actual
        // values of intermediate type if present,
        // falling back on the desired/predicted types otherwise.
        let in_sockets: Cow<[nde::node::SocketType]> =
            create_in_sockets(nde_filter, nde_result_or_error, node_id, node.as_ref());
        let out_sockets: Cow<[nde::node::SocketType]> =
            create_out_sockets(nde_result_or_error, node_id, node.as_ref());

        let socket_row_count = out_sockets.len().max(in_sockets.len());

        // Iterate over the collected input and output types,
        // and create rows containing appropriately-styled sockets.
        let mut socket_rows: Vec<iced::Element<'_, message::Message>> =
            Vec::with_capacity(socket_row_count);
        for row_num in 0..socket_row_count {
            let socket_id = SocketId(row_num);
            let (in_socket, out_socket) =
                if row_num < in_sockets.len() && row_num < out_sockets.len() {
                    // Both input and output pin at this row
                    (Some(in_sockets[row_num]), Some(out_sockets[row_num]))
                } else if row_num < in_sockets.len() {
                    // Only input pin at this row
                    (Some(in_sockets[row_num]), None)
                } else if row_num < out_sockets.len() {
                    // Only output pin
                    (None, Some(out_sockets[row_num]))
                } else {
                    unreachable!();
                };

            let row = make_pin_row(socket_id, in_socket, out_socket);
            socket_rows.push(row);
        }

        let pin_list = iced::widget::column(socket_rows).spacing(4);
        let content_style = iced_nodegraph::NodeContentStyle::process(&style::samaku_theme());
        let title =
            iced::widget::container(iced::widget::text(node.name())).padding(iced::Padding {
                top: 4.0,
                bottom: 4.0,
                left: 8.0,
                right: 8.0,
            });
        let title_bar = iced_nodegraph::node_header(
            title,
            content_style.title_background,
            content_style.corner_radius,
            content_style.border_width,
        );
        let node_element: iced::Element<'_, message::Message> = iced::widget::column![
            title_bar,
            iced::widget::container(visual_node.node.content(nde_filter_id, node_id))
                .padding([10, 12]),
            iced::widget::container(pin_list).padding([10, 12])
        ]
        .width(200.0)
        .into();

        let node_border_colour = match *nde_result_or_error {
            Ok(ref nde_result) => match nde_result.intermediates.get(node_index) {
                Some(&NodeState::Inactive) => style::SAMAKU_INACTIVE,
                Some(&NodeState::Active(_)) => style::SAMAKU_PRIMARY,
                Some(&NodeState::Error(_)) => style::SAMAKU_DESTRUCTIVE,
                None => panic!("intermediate node not found"),
            },
            Err(_) => {
                // If there was an error, make all nodes appear red
                style::SAMAKU_DESTRUCTIVE
            }
        };

        let node_style = iced_nodegraph::NodeConfig::new().border_color(node_border_colour);
        graph.push_node_styled(node_id, visual_node.position, node_element, node_style);
    }
}

fn create_out_sockets<'a>(
    nde_result_or_error: &Result<NdeResult, NdeError>,
    node_id: NodeId,
    node: &'a dyn nde::Node,
) -> Cow<'a, [nde::node::SocketType]> {
    // For the outputs, just iterate and merge one list
    // with the other. But first, we need to check the preconditions,
    // like whether the compilation was successful and whether the current node
    // is even active
    match *nde_result_or_error {
        Ok(ref nde_result) => match &nde_result.intermediates[node_id.0] {
            &NodeState::Active(ref socket_values) => {
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
    node_id: NodeId,
    node: &'a dyn nde::Node,
) -> Cow<'a, [nde::node::SocketType]> {
    // For the inputs, we use the same general logic as before,
    // but instead of checking the node's sockets directly,
    // we need to check the nodes connecting into it
    match *nde_result_or_error {
        Ok(ref nde_result) => {
            // Note that here we don't check the active node's state,
            // because we can still make a judgment about its input types
            // even if it is inactive or errored.
            //
            // Initialise the result with the desired inputs,
            // overwriting later
            let mut merged: Vec<nde::node::SocketType> = Vec::from(node.desired_inputs());

            // Iterate over the nodes that connect into our node
            for (previous, next_socket_index) in nde_filter.graph.iter_previous(node_id) {
                // Check whether the previous node is active
                // (otherwise, ignore it)
                if let &NodeState::Active(ref previous_socket_values) =
                    &nde_result.intermediates[previous.node_index.0]
                {
                    // Check whether the previous node has returned
                    // a type-representable value at the given socket position
                    if let Some(actual_type) = previous_socket_values
                        .get(previous.socket_index.0)
                        .and_then(nde::node::SocketValue::as_type)
                    {
                        merged[next_socket_index.0] = actual_type;
                    }
                }
            }

            Cow::Owned(merged)
        }
        Err(_) => Cow::Borrowed(node.desired_inputs()),
    }
}

fn create_connections(
    graph: &mut NodeGraph,
    nde_filter: &nde::Filter,
    nde_result_or_error: &Result<NdeResult, NdeError>,
) {
    let color = match *nde_result_or_error {
        Ok(_) => style::SAMAKU_PRIMARY,
        Err(_) => style::SAMAKU_DESTRUCTIVE,
    };

    let edge_config = iced_nodegraph::EdgeConfig::new().solid_color(color);

    for (next, previous) in &nde_filter.graph.connections {
        let from =
            iced_nodegraph::PinRef::new(previous.node_index, PinId::output(previous.socket_index));
        let to = iced_nodegraph::PinRef::new(next.node_index, PinId::input(next.socket_index));
        graph.push_edge_styled(from, to, edge_config.clone());
    }
}

fn view_graph<'a>(
    pane_state: &'a State,
    nde_filter_id: subtitle::ExtradataId,
    nde_filter: &nde::Filter,
    nde_result_or_error: &Result<NdeResult, NdeError>,
    graph_box: Box<NodeGraph<'a>>,
) -> iced::Element<'a, message::Message> {
    let menu_bar = iced_aw::menu::MenuBar::new(add_menu(nde_filter_id))
        .width(180)
        .height(32);

    let unassign_button = iced::widget::button(iced::widget::text("Unassign")).on_press(
        message::Message::UnassignFilterFromSelectedEvents(nde_filter_id),
    );

    let name_box = iced::widget::text_input("Filter name", &nde_filter.name)
        .on_input(move |name| message::Message::SetFilterName(nde_filter_id, name))
        .padding(5.0)
        .width(iced::Length::Fixed(200.0));

    let error_tooltip = view_error(pane_state, nde_result_or_error);

    let bottom_bar = iced::widget::container(
        iced::widget::row![
            menu_bar,
            unassign_button,
            name_box,
            iced::widget::space::horizontal(),
            error_tooltip,
        ]
        .spacing(5.0)
        .align_y(iced::Alignment::Center),
    )
    .padding(5.0);

    let graph: NodeGraph = *graph_box;
    iced::widget::column![graph, view::separator(), bottom_bar].into()
}

fn view_non_selected(
    self_pane: super::Pane,
    pane_state: &'_ State,
    multi_warning: bool,
) -> iced::Element<'_, message::Message> {
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
            .map(|filter_ref| message::Message::AssignFilterToSelectedEvents(filter_ref.index)),
    );
    let create_button = iced::widget::button(iced::widget::text("Create new"))
        .on_press(message::Message::CreateEmptyFilter);
    let delete_button = iced::widget::button(iced::widget::text("Delete")).on_press_maybe(
        pane_state
            .selected_filter
            .as_ref()
            .map(|filter_ref| message::Message::DeleteFilter(filter_ref.index)),
    );

    let warning_text = if multi_warning {
        iced::widget::text("To edit a filter, select only one event that has it assigned.").style(
            |_theme| iced::widget::text::Style {
                color: Some(style::SAMAKU_PRIMARY),
            },
        )
    } else {
        iced::widget::text("")
    };

    iced::widget::column![
        iced::widget::text("Filters").size(20),
        selection_list,
        iced::widget::row![assign_button, create_button, delete_button].spacing(5),
        warning_text,
    ]
    .spacing(5)
    .into()
}

fn view_error<'a>(
    pane_state: &'a State,
    nde_result_or_error: &Result<NdeResult, NdeError>,
) -> iced::Element<'a, message::Message> {
    let error_state = match *nde_result_or_error {
        Ok(ref result) => {
            let num_selected_nodes = pane_state.selected_nodes.len();
            if num_selected_nodes == 1 {
                format_error(result, pane_state.selected_nodes.first().unwrap().0, None)
            } else {
                // multiple or no nodes selected
                let error_count = result
                    .intermediates
                    .iter()
                    .filter(|node_state| matches!(node_state, NodeState::Error(_)))
                    .count();

                if error_count > 0 {
                    if num_selected_nodes > 1 {
                        // Multiple nodes selected
                        GraphErrorState::message(
                            "Multiple nodes selected",
                            "Select a node to view details about the error that occurred in the node.",
                        )
                    } else {
                        // No nodes selected, try to find the first error, and hide the rest
                        let first_error_index = result.first_error_index.expect(
                            "node error count greater than 0, but no first error index was set",
                        );
                        format_error(result, first_error_index, Some(error_count))
                    }
                } else {
                    GraphErrorState::none()
                }
            }
        }
        Err(NdeError::CycleInGraph) => {
            GraphErrorState::error("Cycle detected!", "A cycle was detected in the graph.")
        }
    };

    let error_message =
        iced::widget::text(error_state.message).style(move |_theme| iced::widget::text::Style {
            color: Some(error_state.color),
        });

    iced::widget::tooltip(
        error_message,
        iced::widget::container(iced::widget::text(error_state.tooltip))
            .padding(10)
            .style(iced::widget::container::rounded_box),
        iced::widget::tooltip::Position::Top,
    )
    .into()
}

fn format_error(
    result: &NdeResult,
    error_index: usize,
    error_count_option: Option<usize>,
) -> GraphErrorState {
    let first_error = result
        .intermediates
        .get(error_index)
        .expect("no node state present at error index");
    if let NodeState::Error(ref error) = *first_error {
        let first_message = error.to_string();
        let causes = error
            .chain()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
            .join("\nReason: ");
        let message = if let Some(error_count) = error_count_option
            && error_count > 1
        {
            let remaining_error_count = error_count - 1;
            let maybe_s = if remaining_error_count == 1 { "" } else { "s" };
            format!("{first_message}, and {remaining_error_count} further error{maybe_s}")
        } else {
            first_message
        };

        GraphErrorState::error(message, causes)
    } else {
        GraphErrorState::none()
    }
}

struct GraphErrorState {
    message: String,
    tooltip: String,
    color: iced::Color,
}

impl GraphErrorState {
    fn error<S1: Into<String>, S2: Into<String>>(message: S1, tooltip: S2) -> Self {
        Self {
            message: message.into(),
            tooltip: tooltip.into(),
            color: style::SAMAKU_DESTRUCTIVE,
        }
    }

    fn message<S1: Into<String>, S2: Into<String>>(message: S1, tooltip: S2) -> Self {
        Self {
            message: message.into(),
            tooltip: tooltip.into(),
            color: style::SAMAKU_TEXT,
        }
    }

    fn none() -> Self {
        Self::message("", "")
    }
}

fn make_pin<'a>(
    role: &SocketRole,
    socket_id: SocketId,
    socket_type: nde::node::SocketType,
) -> Option<iced_nodegraph::NodePin<'a, PinId, message::Message, iced::Theme, iced::Renderer>> {
    const BLOB_RADIUS: f32 = 7.0;

    // The style of the blob is not determined by a style sheet, but by properties of the `Socket`
    // itself.
    let (_blob_border_radius, blob_color, label) = match socket_type {
        nde::node::SocketType::IndividualEvent => (0.0, iced::Color::from_rgb(1.0, 1.0, 1.0), ""),
        nde::node::SocketType::MultipleEvents => (0.0, style::SAMAKU_PRIMARY, ""),
        nde::node::SocketType::AnyEvents => (0.0, style::SAMAKU_BACKGROUND, ""),
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

    // TODO: figure out how to apply shape/border radius, and size

    Some(
        iced_nodegraph::node_pin(
            role.side,
            (role.pin_id_func)(socket_id),
            iced::widget::text(label).style(|_| iced::widget::text::Style {
                color: Some(style::SAMAKU_TEXT),
            }),
        )
        .direction(role.direction)
        .color(blob_color),
    )
}

fn make_pin_row<'a>(
    socket_id: SocketId,
    in_socket: Option<nde::node::SocketType>,
    out_socket: Option<nde::node::SocketType>,
) -> iced::Element<'a, message::Message> {
    let in_pin_option =
        in_socket.and_then(|socket_type| make_pin(&SocketRole::IN, socket_id, socket_type));
    let out_pin_option =
        out_socket.and_then(|socket_type| make_pin(&SocketRole::OUT, socket_id, socket_type));

    if let Some(in_pin) = in_pin_option {
        if let Some(out_pin) = out_pin_option {
            // Both pins present
            iced::widget::row![
                iced::widget::container(in_pin)
                    .width(iced::Length::FillPortion(1))
                    .align_x(iced::alignment::Horizontal::Left),
                iced::widget::container(out_pin)
                    .width(iced::Length::FillPortion(1))
                    .align_x(iced::alignment::Horizontal::Right),
            ]
            .into()
        } else {
            // Only in pin
            iced::widget::container(in_pin)
                .width(iced::Length::Fill)
                .align_x(iced::alignment::Horizontal::Left)
                .into()
        }
    } else {
        if let Some(out_pin) = out_pin_option {
            // Only out pin
            iced::widget::container(out_pin)
                .width(iced::Length::Fill)
                .align_x(iced::alignment::Horizontal::Right)
                .into()
        } else {
            // Neither in nor out pin
            iced::widget::container("").width(iced::Length::Fill).into()
        }
    }
}

fn menu_item(
    label: &'_ str,
    nde_filter_id: subtitle::ExtradataId,
    node_constructor: nde::node::Constructor,
) -> iced_aw::menu::Item<'_, message::Message, iced::Theme, iced::Renderer> {
    view::menu::item(
        label,
        message::Message::AddNode(nde_filter_id, node_constructor),
    )
}

fn sub_menu<'a>(
    label: &'a str,
    children: Vec<iced_aw::menu::Item<'a, message::Message, iced::Theme, iced::Renderer>>,
) -> iced_aw::menu::Item<'a, message::Message, iced::Theme, iced::Renderer> {
    view::menu::sub_menu(label, message::Message::None, children)
}

fn add_menu<'a>(
    nde_filter_id: subtitle::ExtradataId,
) -> Vec<iced_aw::menu::Item<'a, message::Message, iced::Theme, iced::Renderer>> {
    vec![iced_aw::menu::Item::with_menu(
        iced::widget::button(iced::widget::text("Add node")).on_press(message::Message::None),
        iced_aw::menu::Menu::new(children_from_shell_tree(&SHELL_TREE, nde_filter_id))
            .width(iced::Length::Fixed(150.0)),
    )]
}

fn children_from_shell_tree(
    tree: &'_ ShellMap,
    nde_filter_id: subtitle::ExtradataId,
) -> Vec<iced_aw::menu::Item<'_, message::Message, iced::Theme, iced::Renderer>> {
    let mut children = vec![];

    for (name, child) in tree {
        match *child {
            MenuShell::Item(ref constructor) => {
                children.push(menu_item(name.as_str(), nde_filter_id, *constructor));
            }
            MenuShell::SubMenu(ref sub_tree) => {
                children.push(sub_menu(
                    name.as_str(),
                    children_from_shell_tree(sub_tree, nde_filter_id),
                ));
            }
        }
    }

    children
}

type ShellMap = BTreeMap<String, MenuShell>;

static SHELL_TREE: LazyLock<ShellMap> = LazyLock::new(collect_menu);

#[derive(Debug)]
enum MenuShell {
    Item(nde::node::Constructor),
    SubMenu(ShellMap),
}

/// Collect the `inventory` of node shells and create a menu tree from it, which will later need
/// to be converted into iced_aw widgets.
fn collect_menu() -> ShellMap {
    let mut menu: ShellMap = BTreeMap::new();

    for node_shell in inventory::iter::<nde::node::Shell> {
        if node_shell.menu_path.is_empty() {
            continue;
        }

        match collect_internal_recursive(&mut menu, node_shell.menu_path, node_shell.constructor) {
            Ok(()) => {}
            Err(CollectError::DuplicateItem) => panic!(
                "Found duplicate item while collecting node with menu path: {:?}",
                node_shell.menu_path
            ),
            Err(CollectError::ItemOverSubMenu) => panic!(
                "Tried to insert node item with menu path {:?}, but found an existing sub menu",
                node_shell.menu_path
            ),
            Err(CollectError::SubMenuOverItem) => panic!(
                "Tried to insert sub menu for node with menu path {:?}, but found an existing item",
                node_shell.menu_path
            ),
        }
    }

    menu
}

fn collect_internal_recursive(
    menu: &mut ShellMap,
    path: &[&str],
    constructor: nde::node::Constructor,
) -> Result<(), CollectError> {
    assert!(!path.is_empty(), "menu path must not be empty");

    let first_path_element = path[0];

    if path.len() == 1 {
        // Only the last element remains, which must be inserted as an item.
        match menu.get(first_path_element) {
            Some(&MenuShell::Item(_)) => return Err(CollectError::DuplicateItem),
            Some(&MenuShell::SubMenu(_)) => return Err(CollectError::ItemOverSubMenu),
            None => {
                menu.insert(first_path_element.to_owned(), MenuShell::Item(constructor));
            }
        }

        Ok(())
    } else {
        // Insert the first element as a sub menu.
        let sub_menu = match menu.get_mut(first_path_element) {
            Some(&mut MenuShell::Item(_)) => return Err(CollectError::SubMenuOverItem),
            Some(&mut MenuShell::SubMenu(ref mut sub_menu)) => sub_menu,
            None => {
                menu.insert(
                    first_path_element.to_owned(),
                    MenuShell::SubMenu(BTreeMap::new()),
                );
                let Some(&mut MenuShell::SubMenu(ref mut sub_menu)) =
                    menu.get_mut(first_path_element)
                else {
                    panic!();
                };
                sub_menu
            }
        };
        collect_internal_recursive(sub_menu, &path[1..], constructor)
    }
}

enum CollectError {
    DuplicateItem,
    ItemOverSubMenu,
    SubMenuOverItem,
}
