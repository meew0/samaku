use std::borrow::Cow;

use crate::{media, nde};

#[must_use]
pub fn trivial<'a>(event: &'a super::Event) -> super::Event<'a> {
    super::Event {
        start: event.start,
        duration: event.duration,
        layer_index: event.layer_index,
        style_index: event.style_index,
        margins: event.margins,
        text: Cow::Borrowed(&event.text),
        actor: Cow::Borrowed(&event.actor),
        effect: Cow::Borrowed(&event.effect),
        event_type: super::EventType::Dialogue,
        extradata_ids: vec![],
    }
}

/// Contains all extra data that may be used by NDE filters and must thus be specified
/// for non-trivial compilations, such as the video frame rate.
///
/// Also specifies some utility methods.
#[derive(Clone)]
pub struct Context<'a> {
    pub frame_rate: &'a media::FrameRate,
    pub source_event: Option<&'a super::Event<'static>>,
    pub styles: &'a super::StyleList,
    pub motion_tracks: Option<&'a media::motion::TrackList>,

    /// The `PlayRes` defined in the ASS file header,
    /// or the video resolution, if none is defined.
    pub playback_resolution: super::Resolution,

    /// The `LayoutRes` defined in the ASS file header,
    /// or the video resolution, if none is defined.
    pub layout_resolution: super::Resolution,
}

impl Context<'_> {
    /// Find the style of a given event.
    #[must_use]
    pub fn get_event_style(&self, event: &nde::Event) -> &super::Style {
        self.styles.get_or_default(event.style_index)
    }
}

/// Utility macro to construct a `Context` based on global state
/// without borrowing everything at once (to allow simultaneously
/// mutably borrowing other parts of the same global state).
macro_rules! context {
    ($global_state:expr, $source_event:expr) => {
        subtitle::compile::Context {
            frame_rate: if let &Some(ref video_metadata) = &$global_state.video_metadata {
                &video_metadata.frame_rate
            } else {
                &*crate::media::UNLOADED_FRAMERATE
            },
            source_event: $source_event,
            styles: &$global_state.subtitles.styles,
            motion_tracks: Some(&$global_state.motion_tracks),
            playback_resolution: $global_state.subtitles.script_info.playback_resolution,
            layout_resolution: $global_state.effective_layout_resolution(),
        }
    };
}

pub(crate) use context;

/// Applies the given `filter` to the given `event`, and returns the resulting events plus certain
/// intermediate values. The `counter` is counted up for every created event and used as its read
/// index.
///
/// # Errors
/// Returns [`NdeError::CycleInGraph`] if the graph contains a cycle.
///
/// # Panics
/// Panics if the filter's output node does not provide a [`SocketValue::CompiledEvents`].
pub fn nde<'a>(
    filter: &nde::graph::Graph,
    context: &Context<'a>,
) -> Result<NdeResult<'a>, NdeError> {
    // The intermediates may borrow from the context and thus exist within the confines of this function;
    // the node states only contain “metadata” and do not borrow from anything.
    let mut intermediates: Vec<Option<nde::node::SocketValues>> = vec![None; filter.nodes.len()];
    let mut node_states: Vec<NodeState> = Vec::with_capacity(filter.nodes.len());

    // TODO: we cannot use a vec! macro or resize here because NodeStates aren't cloneable
    for _ in 0..filter.nodes.len() {
        node_states.push(NodeState::Inactive);
    }

    let mut first_error_index: Option<usize> = None;

    let mut process_queue = match filter.dfs() {
        nde::graph::DfsResult::ProcessQueue(queue) => queue,
        nde::graph::DfsResult::CycleFound => return Err(NdeError::CycleInGraph),
    };

    // Go through the process queue and process the individual nodes
    while let Some(node_index) = process_queue.pop_front() {
        let node = &filter.nodes[node_index.0].node;
        let desired_inputs = node.desired_inputs();
        let mut inputs: nde::node::SocketValues =
            smallvec::smallvec![nde::node::SocketValue::None; desired_inputs.len()];

        // Find connections that would theoretically supply inputs to the current node,
        // check whether those nodes are active, and if they are, supply the inputs
        for (previous, next_socket_index) in filter.iter_previous(node_index) {
            if let &Some(ref prev_intermediate) = &intermediates[previous.node_index.0] {
                // TODO: maybe we can avoid always cloning the prev_intermediate,
                // since in the common case it will only be used once and thus can be taken out of the
                // prev_intermediate.
                // In theory we know how many times it will be needed (by counting the outgoing connections
                // in the graph).
                inputs[next_socket_index.0] = prev_intermediate[previous.socket_index.0].clone();
            }
        }

        // Run the node and store the results.
        // Note that this is still done even if some of the previous nodes are inactive/errored.
        // This means that the current node will likely error as well, but that is ok
        let (intermediate, node_state) = match node.run(inputs, context) {
            Ok(outputs) => {
                let active = outputs
                    .iter()
                    .map(nde::node::SocketValue::to_result)
                    .collect();
                (Some(outputs), NodeState::Active(active))
            }
            Err(err) => {
                if first_error_index.is_none() {
                    first_error_index = Some(node_index.0);
                }
                (None, NodeState::Error(err))
            }
        };

        intermediates[node_index.0] = intermediate;
        node_states[node_index.0] = node_state;
    }

    // Get the “output” of the output node
    if let &mut Some(ref mut output_node_outputs) = &mut intermediates[0] {
        let first_output = output_node_outputs.swap_remove(0);

        match first_output {
            nde::node::SocketValue::CompiledEvents(events) => Ok(NdeResult {
                events: Some(events),
                node_states,
                first_error_index,
            }),
            _ => {
                panic!("the output of the output node should be a CompiledEvents socket value")
            }
        }
    } else {
        Ok(NdeResult {
            events: None,
            node_states,
            first_error_index,
        })
    }
}

pub struct NdeResult<'a> {
    pub events: Option<Vec<super::Event<'a>>>,
    pub node_states: Vec<NodeState>,
    pub first_error_index: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum NdeError {
    #[error("Cycle in graph detected")]
    CycleInGraph,
}

pub enum NodeState {
    Inactive,
    Active(smallvec::SmallVec<nde::node::SocketResult, 2>),
    Error(anyhow::Error),
}

impl std::fmt::Debug for NodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            NodeState::Inactive => write!(f, "Inactive"),
            NodeState::Active(_) => write!(f, "Active"),
            NodeState::Error(_) => write!(f, "Error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use assert_matches2::assert_matches;

    use super::super::*;
    use super::*;

    #[test]
    fn compile_nde() {
        let filter = nde::graph::Graph::from_single_intermediate(Box::new(nde::node::Italic {}));

        assert_eq!(
            filter.dfs(),
            nde::graph::DfsResult::ProcessQueue(VecDeque::from([
                nde::graph::NodeId(2),
                nde::graph::NodeId(1),
                nde::graph::NodeId(0)
            ]))
        );

        let event = Event {
            start: StartTime(0),
            duration: Duration(1000),
            text: Cow::Owned("This text will become italic".to_owned()),
            ..Default::default()
        };

        let style_list = StyleList::new();
        let context = Context {
            frame_rate: &media::FrameRate::f24(),
            source_event: Some(&event),
            styles: &style_list,
            motion_tracks: None,
            playback_resolution: Resolution::FULL_HD,
            layout_resolution: Resolution::FULL_HD,
        };

        let result = nde(&filter, &context).expect("there should be no error");

        for node_state in &result.node_states {
            assert_matches!(node_state, &NodeState::Active { .. });
        }

        if let &NodeState::Active(ref socket_values) = &result.node_states[1] {
            assert_matches!(
                socket_values[0].value_type,
                Some(nde::node::SocketType::Event)
            );
        }

        let events = result.events.expect("there should be output events");
        assert_eq!(events.len(), 1);

        let first_event = &events[0];
        assert_eq!(first_event.text, "{\\i1}This text will become italic");
    }
}
