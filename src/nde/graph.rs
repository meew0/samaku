use std::collections::VecDeque;

use super::node::Node;

/// A directed acyclic graph of nodes, representing a NDE filter as a whole.
/// Stored as an adjacency
#[derive(Debug, Clone)]
pub struct Graph {
    pub nodes: Vec<VisualNode>,
    pub connectors: Vec<Vec<Connector>>,
}

impl Default for Graph {
    fn default() -> Self {
        Graph {
            nodes: vec![VisualNode {
                node: Node::Output,
                position: DisplayPosition { x: 0.0, y: 0.0 },
            }],
            connectors: vec![vec![]; 1],
        }
    }
}

impl Graph {
    pub fn verify_output_node(&self) {
        let output_node = &self.nodes[0];
        if output_node.node != Node::Output {
            panic!("first node in graph must be the output node");
        }
    }

    pub fn dfs(&self) -> DfsResult {
        let mut process_queue: VecDeque<usize> = VecDeque::new();
        let mut seen = vec![false; self.nodes.len()];
        let mut cycle_detector: CycleDetector = CycleDetector::new(self.nodes.len());

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

        for connector in self.connectors[next].iter() {
            let prev = connector.previous_node_index;
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

#[derive(Debug, Clone, PartialEq)]
pub enum DfsResult {
    CycleFound,
    ProcessQueue(VecDeque<usize>),
}

struct CycleDetector {
    matrix: Vec<bool>,
    n: usize,
}

impl CycleDetector {
    pub fn new(n: usize) -> Self {
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

    pub fn set_parent(&mut self, parent: usize, child: usize) -> CycleFound {
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
    pub fn cycle_found(&self) -> bool {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct VisualNode {
    pub node: Node,
    pub position: DisplayPosition,
}

#[derive(Debug, Clone, Copy)]
pub struct DisplayPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct Connector {
    pub previous_node_index: usize,
    pub previous_socket_index: usize,
    pub next_socket_index: usize,
}
