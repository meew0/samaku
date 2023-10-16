use std::borrow::Cow;

use crate::{media, nde};

pub fn trivial<'a>(sline: &'a super::Sline, counter: &mut i32) -> super::CompiledEvent<'a> {
    let event = super::CompiledEvent {
        start: sline.start,
        duration: sline.duration,
        layer_index: sline.layer_index,
        style_index: sline.style_index,
        margins: sline.margins,
        text: Cow::from(sline.text.as_str()),
        read_order: *counter,
        name: Cow::from(""),
        effect: Cow::from(""),
    };

    *counter += 1;
    event
}

/// Applies the given `filter` to the given `sline`, and returns the resulting events plus certain
/// intermediate values. The `counter` is counted up for every created event and used as its read
/// index.
///
/// # Errors
/// Returns [`NdeError::CycleInGraph`] if the graph contains a cycle.
///
/// # Panics
/// Panics if the filter's output node does not provide a [`SocketValue::CompiledEvents`].
pub fn nde<'a, 'b>(
    sline: &'a super::Sline,
    filter: &'b nde::graph::Graph,
    frame_rate: media::FrameRate,
    counter: &mut i32,
) -> Result<NdeResult<'a, 'b>, NdeError> {
    let mut intermediates: Vec<NodeState> = vec![NodeState::Inactive; filter.nodes.len()];
    let mut process_queue = match filter.dfs() {
        nde::graph::DfsResult::ProcessQueue(queue) => queue,
        nde::graph::DfsResult::CycleFound => return Err(NdeError::CycleInGraph),
    };
    let sline_value = nde::node::SocketValue::Sline(sline);
    let frame_rate_value = nde::node::SocketValue::FrameRate(frame_rate);

    // Go through the process queue and process the individual nodes
    while let Some(node_index) = process_queue.pop_front() {
        let node = &filter.nodes[node_index].node;
        let desired_inputs = node.desired_inputs();
        let mut inputs: Vec<&nde::node::SocketValue> =
            vec![&nde::node::SocketValue::None; desired_inputs.len()];

        // Pass inputs to leaf nodes
        for (i, desired_input) in desired_inputs.iter().enumerate() {
            if let nde::node::SocketType::LeafInput(desired_leaf_input) = desired_input {
                match desired_leaf_input {
                    nde::node::LeafInputType::Sline => {
                        inputs[i] = &sline_value;
                    }
                    nde::node::LeafInputType::FrameRate => {
                        inputs[i] = &frame_rate_value;
                    }
                }
            }
        }

        // Find connections that would theoretically supply inputs to the current node,
        // check whether those nodes are active, and if they are, supply the inputs
        for (previous, next_socket_index) in filter.iter_previous(node_index) {
            if let NodeState::Active(prev_cache) = &intermediates[previous.node_index] {
                inputs[next_socket_index] = &prev_cache[previous.socket_index];
            }
        }

        // Run the node and store the results.
        // Note that this is still done even if some of the previous nodes are inactive/errored.
        // This means that the current node will likely error as well, but that is ok
        intermediates[node_index] = match node.run(&inputs) {
            Ok(outputs) => NodeState::Active(outputs),
            Err(_) => NodeState::Error,
        }
    }

    // Get the “output” of the output node
    match &mut intermediates[0] {
        NodeState::Active(ref mut output_node_outputs) => {
            let first_output = output_node_outputs.swap_remove(0);

            match first_output {
                nde::node::SocketValue::CompiledEvents(mut events) => {
                    for event in &mut events {
                        event.read_order = *counter;
                        *counter += 1;
                    }

                    Ok(NdeResult {
                        events: Some(events),
                        intermediates,
                    })
                }
                _ => {
                    panic!("the output of the output node should be a CompiledEvents socket value")
                }
            }
        }
        _ => Ok(NdeResult {
            events: None,
            intermediates,
        }),
    }
}

pub struct NdeResult<'a, 'b> {
    pub events: Option<Vec<super::CompiledEvent<'a>>>,
    pub intermediates: Vec<NodeState<'b>>,
}

#[derive(Debug)]
pub enum NdeError {
    CycleInGraph,
}

#[derive(Debug, Clone)]
pub enum NodeState<'a> {
    Inactive,
    Active(Vec<nde::node::SocketValue<'a>>),
    Error,
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
            nde::graph::DfsResult::ProcessQueue(VecDeque::from([2, 1, 0]))
        );

        let sline = Sline {
            start: StartTime(0),
            duration: Duration(1000),
            text: "This text will become italic".to_string(),
            ..Default::default()
        };

        let mut counter = 0;

        let result = nde(
            &sline,
            &filter,
            media::FrameRate {
                numerator: 24,
                denominator: 1,
            },
            &mut counter,
        )
        .expect("there should be no error");

        for node_state in &result.intermediates {
            assert_matches!(node_state, NodeState::Active { .. });
        }

        if let NodeState::Active(socket_values) = &result.intermediates[1] {
            assert_matches!(
                &socket_values[0],
                nde::node::SocketValue::IndividualEvent { .. }
            );
        }

        let events = result.events.expect("there should be output events");
        assert_eq!(events.len(), 1);

        let first_event = &events[0];
        assert_eq!(first_event.text, "{\\i1}This text will become italic");
    }
}
