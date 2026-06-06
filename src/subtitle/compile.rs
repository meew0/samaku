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
        self.styles.get(event.style_index)
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
pub fn nde<'a, 'b>(
    filter: &'b nde::graph::Graph,
    context: &Context<'a>,
) -> Result<NdeResult<'a, 'b>, NdeError> {
    let mut intermediates: Vec<NodeState> = Vec::with_capacity(filter.nodes.len());
    // we cannot use a vec! macro or resize here because NodeStates aren't cloneable
    for _ in &filter.nodes {
        intermediates.push(NodeState::Inactive);
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
        let mut inputs: Vec<&nde::node::SocketValue> =
            vec![&nde::node::SocketValue::None; desired_inputs.len()];

        // Find connections that would theoretically supply inputs to the current node,
        // check whether those nodes are active, and if they are, supply the inputs
        for (previous, next_socket_index) in filter.iter_previous(node_index) {
            if let &NodeState::Active(ref prev_cache) = &intermediates[previous.node_index.0] {
                inputs[next_socket_index.0] = &prev_cache[previous.socket_index.0];
            }
        }

        // Run the node and store the results.
        // Note that this is still done even if some of the previous nodes are inactive/errored.
        // This means that the current node will likely error as well, but that is ok
        intermediates[node_index.0] = match node.run(&inputs, context) {
            Ok(outputs) => NodeState::Active(outputs),
            Err(err) => {
                if first_error_index.is_none() {
                    first_error_index = Some(node_index.0);
                }
                NodeState::Error(err)
            }
        }
    }

    // Get the “output” of the output node
    match &mut intermediates[0] {
        &mut NodeState::Active(ref mut output_node_outputs) => {
            let first_output = output_node_outputs.swap_remove(0);

            match first_output {
                nde::node::SocketValue::CompiledEvents(events) => Ok(NdeResult {
                    events: Some(events),
                    intermediates,
                    first_error_index,
                }),
                _ => {
                    panic!("the output of the output node should be a CompiledEvents socket value")
                }
            }
        }
        _ => Ok(NdeResult {
            events: None,
            intermediates,
            first_error_index,
        }),
    }
}

pub struct NdeResult<'a, 'b> {
    pub events: Option<Vec<super::Event<'a>>>,
    pub intermediates: Vec<NodeState<'b>>,
    pub first_error_index: Option<usize>,
}

#[derive(Debug)]
pub enum NdeError {
    CycleInGraph,
}

#[derive(Debug)]
pub enum NodeState<'a> {
    Inactive,
    Active(Vec<nde::node::SocketValue<'a>>),
    Error(anyhow::Error),
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

        for node_state in &result.intermediates {
            assert_matches!(node_state, &NodeState::Active { .. });
        }

        if let &NodeState::Active(ref socket_values) = &result.intermediates[1] {
            assert_matches!(
                &socket_values[0],
                &nde::node::SocketValue::IndividualEvent { .. }
            );
        }

        let events = result.events.expect("there should be output events");
        assert_eq!(events.len(), 1);

        let first_event = &events[0];
        assert_eq!(first_event.text, "{\\i1}This text will become italic");
    }
}
