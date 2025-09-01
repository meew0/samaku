use std::collections::{HashMap, VecDeque};

use super::node;

/// A directed acyclic graph of nodes, representing a NDE filter as a whole.
/// Stored as an adjacency map.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Graph {
    pub nodes: Vec<VisualNode>,
    pub connections: HashMap<NextEndpoint, PreviousEndpoint>,
}

impl Default for Graph {
    fn default() -> Self {
        Graph {
            nodes: vec![VisualNode {
                node: Box::new(node::Output {}),
                position: iced::Point { x: 0.0, y: 0.0 },
            }],
            connections: HashMap::new(),
        }
    }
}

impl Graph {
    /// Returns a graph that contains an input node connected to an output node, with no further
    /// processing done.
    #[must_use]
    pub fn identity() -> Self {
        let mut connections = HashMap::new();
        connections.insert(
            NextEndpoint {
                node_index: 0,
                socket_index: 0,
            },
            PreviousEndpoint {
                node_index: 1,
                socket_index: 0,
            },
        );

        Self {
            nodes: vec![
                VisualNode {
                    node: Box::new(node::Output {}),
                    position: iced::Point { x: 400.0, y: 100.0 },
                },
                VisualNode {
                    node: Box::new(node::InputEvent {}),
                    position: iced::Point { x: 100.0, y: 100.0 },
                },
            ],
            connections,
        }
    }

    /// Returns a basic graph that contains an input node, an intermediate filter node,
    /// and an output node. Useful for testing.
    #[must_use]
    pub fn from_single_intermediate(intermediate: Box<dyn node::Node>) -> Self {
        let mut connections = HashMap::new();
        connections.insert(
            NextEndpoint {
                node_index: 0,
                socket_index: 0,
            },
            PreviousEndpoint {
                node_index: 1,
                socket_index: 0,
            },
        );
        connections.insert(
            NextEndpoint {
                node_index: 1,
                socket_index: 0,
            },
            PreviousEndpoint {
                node_index: 2,
                socket_index: 0,
            },
        );

        Self {
            nodes: vec![
                VisualNode {
                    node: Box::new(node::Output {}),
                    position: iced::Point { x: 600.0, y: 0.0 },
                },
                VisualNode {
                    node: intermediate,
                    position: iced::Point { x: 300.0, y: 0.0 },
                },
                VisualNode {
                    node: Box::new(node::InputEvent {}),
                    position: iced::Point { x: 0.0, y: 0.0 },
                },
            ],
            connections,
        }
    }

    /// Asserts that the first node in the graph is an output node.
    ///
    /// # Panics
    /// Panics if it is not.
    pub fn assert_output_node(&self) {
        let output_node = &self.nodes[0];
        assert!(
            output_node.node.predicted_outputs().is_empty(),
            "first node in graph must be an output node"
        );
    }

    pub fn connect(&mut self, next: NextEndpoint, previous: PreviousEndpoint) {
        self.connections.insert(next, previous);
    }

    pub fn disconnect(&mut self, next: NextEndpoint) -> Option<PreviousEndpoint> {
        self.connections.remove(&next)
    }

    /// Iterate over the sockets connecting into the specified node.
    /// Returns tuples `(previous_endpoint, next_socket_index)`
    pub fn iter_previous(
        &self,
        next_node_index: usize,
    ) -> impl Iterator<Item = (&PreviousEndpoint, usize)> {
        self.nodes[next_node_index]
            .node
            .desired_inputs()
            .iter()
            .enumerate()
            .filter_map(move |(socket_index, _)| {
                self.connections
                    .get(&NextEndpoint {
                        node_index: next_node_index,
                        socket_index,
                    })
                    .map(|previous_endpoint| (previous_endpoint, socket_index))
            })
    }

    #[must_use]
    pub fn dfs(&self) -> DfsResult {
        let mut process_queue: VecDeque<usize> = VecDeque::new();
        let mut seen = vec![false; self.nodes.len()];
        let mut cycle_detector = CycleDetector::new(self.nodes.len());

        if self
            .dfs_internal(0, &mut process_queue, &mut seen, &mut cycle_detector)
            .cycle_found()
        {
            return DfsResult::CycleFound;
        }

        DfsResult::ProcessQueue(process_queue)
    }

    fn dfs_internal(
        &self,
        next: usize,
        process_queue: &mut VecDeque<usize>,
        seen: &mut Vec<bool>,
        cycle_detector: &mut CycleDetector,
    ) -> CycleFound {
        seen[next] = true;

        for (previous, _) in self.iter_previous(next) {
            let prev = previous.node_index;
            if cycle_detector.set_parent(next, prev).cycle_found() {
                return CycleFound(true);
            }
            if !seen[prev]
                && self
                    .dfs_internal(prev, process_queue, seen, cycle_detector)
                    .cycle_found()
            {
                return CycleFound(true);
            }
        }

        process_queue.push_back(next);
        CycleFound(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DfsResult {
    CycleFound,
    ProcessQueue(VecDeque<usize>),
}

struct CycleDetector {
    matrix: Vec<bool>,
    n: usize,
}

impl CycleDetector {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            matrix: vec![false; n * n],
            n,
        }
    }

    fn set_ancestor(&mut self, parent: usize, child: usize) -> CycleFound {
        if parent == child {
            return CycleFound(true);
        }

        self.matrix[parent + self.n * child] = true;
        CycleFound(false)
    }

    fn is_ancestor(&self, parent: usize, child: usize) -> bool {
        self.matrix[parent + self.n * child]
    }

    pub(crate) fn set_parent(&mut self, parent: usize, child: usize) -> CycleFound {
        if self.is_ancestor(parent, child) {
            return CycleFound(false); // because we would have detected the cycle before
        }

        self.set_ancestor(parent, child);

        for potential_grandparent in 0..self.n {
            if self.is_ancestor(potential_grandparent, parent)
                && self
                    .set_ancestor(potential_grandparent, child)
                    .cycle_found()
            {
                return CycleFound(true);
            }
        }

        CycleFound(false)
    }
}

struct CycleFound(bool);

impl CycleFound {
    pub(crate) fn cycle_found(&self) -> bool {
        self.0
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VisualNode {
    pub node: Box<dyn node::Node>,

    #[serde(with = "IcedPointDef")]
    pub position: iced::Point,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "iced::Point")]
struct IcedPointDef {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NextEndpoint {
    pub node_index: usize,
    pub socket_index: usize,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct PreviousEndpoint {
    pub node_index: usize,
    pub socket_index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle() {
        let mut graph_with_cycle = Graph::from_single_intermediate(Box::new(node::Italic {}));
        graph_with_cycle.connections.insert(
            NextEndpoint {
                node_index: 2,
                socket_index: 0,
            },
            PreviousEndpoint {
                node_index: 0,
                socket_index: 0,
            },
        );
        let dfs_result = graph_with_cycle.dfs();
        assert_eq!(dfs_result, DfsResult::CycleFound);
    }
}
