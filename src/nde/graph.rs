use super::node;
use std::collections::{HashMap, HashSet, VecDeque};

/// A directed acyclic graph of nodes, representing a NDE filter as a whole.
/// Stored as an adjacency map.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Graph {
    // Invariant: must have at least one node, and the first node must be the output node.
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
                node_index: NodeId(0),
                socket_index: SocketId(0),
            },
            PreviousEndpoint {
                node_index: NodeId(1),
                socket_index: SocketId(0),
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
                node_index: NodeId(0),
                socket_index: SocketId(0),
            },
            PreviousEndpoint {
                node_index: NodeId(1),
                socket_index: SocketId(0),
            },
        );
        connections.insert(
            NextEndpoint {
                node_index: NodeId(1),
                socket_index: SocketId(0),
            },
            PreviousEndpoint {
                node_index: NodeId(2),
                socket_index: SocketId(0),
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

    pub fn connect(&mut self, previous: PreviousEndpoint, next: NextEndpoint) {
        self.connections.insert(next, previous);
    }

    pub fn disconnect(&mut self, next: NextEndpoint) -> Option<PreviousEndpoint> {
        self.connections.remove(&next)
    }

    pub fn delete_nodes(&mut self, to_delete_slice: &[NodeId]) -> Vec<Option<NodeId>> {
        let mut to_delete: HashSet<NodeId> =
            to_delete_slice.iter().copied().collect::<HashSet<_>>();
        if to_delete.contains(&NodeId(0)) {
            to_delete.remove(&NodeId(0));
            println!("tried to delete node 0/output node (not allowed)");
        }

        let new_nodes: Vec<VisualNode> = Vec::with_capacity(self.nodes.len() - to_delete.len());
        let old_nodes = std::mem::replace(&mut self.nodes, new_nodes);

        // Create a mapping from source node IDs to either destination node IDs (if retained) or None (if deleted).
        // In the process, consume the `old_nodes` vec and insert the retained nodes into the new nodes vec
        let mut mapping: Vec<Option<NodeId>> = vec![None; self.nodes.len()];
        for (i, node) in old_nodes.into_iter().enumerate() {
            if to_delete.contains(&NodeId(i)) {
                mapping.insert(i, None);
            } else {
                mapping.insert(i, Some(NodeId(self.nodes.len())));
                self.nodes.push(node);
            }
        }

        // TODO: maybe the hashmap and its allocations can be kept. Probably a very low priority optimization
        let new_connections = HashMap::new();
        let old_connections = std::mem::replace(&mut self.connections, new_connections);

        for (next, previous) in old_connections {
            if let Some(dest_next) = mapping[next.node_index.0]
                && let Some(dest_previous) = mapping[previous.node_index.0]
            {
                self.connections.insert(
                    NextEndpoint {
                        node_index: dest_next,
                        socket_index: next.socket_index,
                    },
                    PreviousEndpoint {
                        node_index: dest_previous,
                        socket_index: previous.socket_index,
                    },
                );
            }
        }

        mapping
    }

    /// Iterate over the sockets connecting into the specified node.
    /// Returns tuples `(previous_endpoint, next_socket_index)`.
    pub fn iter_previous(
        &self,
        next_node_index: NodeId,
    ) -> impl Iterator<Item = (&PreviousEndpoint, SocketId)> {
        self.nodes[next_node_index.0]
            .node
            .desired_inputs()
            .iter()
            .enumerate()
            .filter_map(move |(socket_index, _)| {
                self.connections
                    .get(&NextEndpoint {
                        node_index: next_node_index,
                        socket_index: SocketId(socket_index),
                    })
                    .map(|previous_endpoint| (previous_endpoint, SocketId(socket_index)))
            })
    }

    /// Run depth-first search on the nodes, creating a queue of nodes to process in order.
    ///
    /// # Panics
    /// Panics if structural invariants are violated (node 0 missing/not output node).
    #[must_use]
    pub fn dfs(&self) -> DfsResult {
        // Verify "node 0 == output node" invariant
        let node_0 = self
            .nodes
            .first()
            .expect("nde::Graph invariant violated: node 0 missing");
        assert!(
            node_0.node.is_output(),
            "nde::Graph invariant violated: node 0 is not output node"
        );

        let mut process_queue: VecDeque<NodeId> = VecDeque::new();
        let mut seen = vec![false; self.nodes.len()];
        let mut cycle_detector = CycleDetector::new(self.nodes.len());

        if self
            .dfs_internal(
                NodeId(0),
                &mut process_queue,
                &mut seen,
                &mut cycle_detector,
            )
            .cycle_found()
        {
            return DfsResult::CycleFound;
        }

        DfsResult::ProcessQueue(process_queue)
    }

    fn dfs_internal(
        &self,
        next: NodeId,
        process_queue: &mut VecDeque<NodeId>,
        seen: &mut Vec<bool>,
        cycle_detector: &mut CycleDetector,
    ) -> CycleFound {
        seen[next.0] = true;

        for (previous, _) in self.iter_previous(next) {
            let prev = previous.node_index;
            if cycle_detector.set_parent(next, prev).cycle_found() {
                return CycleFound(true);
            }
            if !seen[prev.0]
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
    ProcessQueue(VecDeque<NodeId>),
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

    fn set_ancestor(&mut self, parent: NodeId, child: NodeId) -> CycleFound {
        if parent == child {
            return CycleFound(true);
        }

        self.matrix[parent.0 + self.n * child.0] = true;
        CycleFound(false)
    }

    fn is_ancestor(&self, parent: NodeId, child: NodeId) -> bool {
        self.matrix[parent.0 + self.n * child.0]
    }

    pub(crate) fn set_parent(&mut self, parent: NodeId, child: NodeId) -> CycleFound {
        if self.is_ancestor(parent, child) {
            return CycleFound(false); // because we would have detected the cycle before
        }

        self.set_ancestor(parent, child);

        for potential_grandparent in 0..self.n {
            if self.is_ancestor(NodeId(potential_grandparent), parent)
                && self
                    .set_ancestor(NodeId(potential_grandparent), child)
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

#[derive(
    Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SocketId(pub usize);

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NextEndpoint {
    pub node_index: NodeId,
    pub socket_index: SocketId,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct PreviousEndpoint {
    pub node_index: NodeId,
    pub socket_index: SocketId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle() {
        let mut graph_with_cycle = Graph::from_single_intermediate(Box::new(node::Italic {}));
        graph_with_cycle.connections.insert(
            NextEndpoint {
                node_index: NodeId(2),
                socket_index: SocketId(0),
            },
            PreviousEndpoint {
                node_index: NodeId(0),
                socket_index: SocketId(0),
            },
        );
        let dfs_result = graph_with_cycle.dfs();
        assert_eq!(dfs_result, DfsResult::CycleFound);
    }
}
