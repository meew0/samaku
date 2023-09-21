use std::borrow::Cow;

use crate::nde;

pub fn trivial<'a>(sline: &'a super::Sline, counter: &mut i32) -> super::ass::Event<'a> {
    let event = super::ass::Event {
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

pub fn nde<'a, 'b>(
    sline: &'a super::Sline,
    filter: &'b nde::graph::Graph,
    counter: &mut i32,
) -> Result<NdeResult<'a, 'b>, NdeError> {
    let mut intermediates: Vec<NodeState> = vec![NodeState::Inactive; filter.nodes.len()];
    let mut process_queue = match filter.dfs() {
        nde::graph::DfsResult::ProcessQueue(queue) => queue,
        nde::graph::DfsResult::CycleFound => return Err(NdeError::CycleInGraph),
    };
    let sline_value = nde::node::SocketValue::Sline(sline);

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
                }
            }
        }

        // Find connections that would theoretically supply inputs to the current node,
        // check whether those nodes are active, and if they are, supply the inputs
        for (socket_index, _) in desired_inputs.iter().enumerate() {
            if let Some(previous) = filter.connections.get(&nde::graph::NextEndpoint {
                node_index,
                socket_index,
            }) {
                if let NodeState::Active(prev_cache) = &intermediates[previous.node_index] {
                    inputs[socket_index] = &prev_cache[previous.socket_index];
                }
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
                    for event in events.iter_mut() {
                        event.read_order = *counter;
                        *counter += 1;
                    }

                    Ok(NdeResult {
                        events,
                        intermediates,
                    })
                }
                _ => {
                    panic!("the output of the output node should be a CompiledEvents socket value")
                }
            }
        }
        _ => Err(NdeError::NoOutput),
    }
}

pub struct NdeResult<'a, 'b> {
    pub events: Vec<super::ass::Event<'a>>,
    pub intermediates: Vec<NodeState<'b>>,
}

#[derive(Debug)]
pub enum NdeError {
    CycleInGraph,
    NoOutput,
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
            layer_index: 0,
            style_index: 0,
            margins: Margins {
                left: 0,
                right: 0,
                vertical: 0,
            },
            text: "This text will become italic".to_string(),

            // This should not matter
            nde_filter_index: Some(1234),
        };

        let mut counter = 0;

        let result = nde(&sline, &filter, &mut counter).expect("there should be no error");

        for node_state in result.intermediates {
            assert!(matches!(node_state, NodeState::Active { .. }));
        }

        assert_eq!(result.events.len(), 1);

        let first_event = &result.events[0];
        assert_eq!(first_event.text, "{\\i1}This text will become italic");
    }
}
