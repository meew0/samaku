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

/// Run an NDE filter graph on the specified sline.
///
/// The filter inside of the sline is ignored, if present;
/// only the `filter_graph` parameter is used.
pub fn nde<'a>(
    sline: &'a super::Sline,
    filter_graph: &nde::graph::Graph,
    counter: &mut i32,
) -> Vec<super::ass::Event<'a>> {
    let mut cache: Vec<Option<Vec<nde::node::SocketValue>>> = vec![None; filter_graph.nodes.len()];
    let mut process_queue = match filter_graph.dfs() {
        nde::graph::DfsResult::ProcessQueue(queue) => queue,
        nde::graph::DfsResult::CycleFound => panic!("there should be no cycles in NDE graphs"),
    };
    let sline_value = nde::node::SocketValue::Sline(sline);

    // Go through the process queue and process the individual nodes
    while let Some(node_index) = process_queue.pop_front() {
        let node = &filter_graph.nodes[node_index].node;
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

        for connector in filter_graph.connectors[node_index].iter() {
            let prev_cache = cache[connector.previous_node_index]
                .as_ref()
                .expect("results from previous node should have been cached");
            inputs[connector.next_socket_index] = &prev_cache[connector.previous_socket_index];
        }

        // Run the node
        let outputs = node.run(&inputs);

        // Cache results
        cache[node_index] = Some(outputs);
    }

    // Get the “output” of the output node
    let output_node_outputs = cache[0]
        .as_mut()
        .expect("the output node should have created a result");
    let first_output = output_node_outputs.swap_remove(0);

    match first_output {
        nde::node::SocketValue::CompiledEvents(mut events) => {
            for event in events.iter_mut() {
                event.read_order = *counter;
            }

            *counter += 1;
            events
        }
        _ => panic!("the output of the output node should be a CompiledEvents socket value"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::super::*;
    use super::*;

    #[test]
    fn compile_nde() {
        let filter_graph = nde::graph::Graph::from_single_intermediate(nde::node::Node::Italic);

        assert_eq!(
            filter_graph.dfs(),
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
            nde_filter: None,
        };

        let mut counter = 0;

        let result = nde(&sline, &filter_graph, &mut counter);
        assert_eq!(result.len(), 1);

        let first_event = &result[0];
        assert_eq!(first_event.text, "{\\i1}This text will become italic");
    }
}
