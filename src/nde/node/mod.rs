use std::fmt::Debug;

pub use input::InputSline;
pub use output::Output;
pub use style_basic::Italic;

use crate::subtitle;

mod input;
mod output;
mod style_basic;

/// Represents a value passed into a socket.
///
/// This struct takes a lifetime argument because it needs to support references to slines, which
/// are awkward to clone. But slines should not be passed around in the node tree, so only leaf
/// nodes should have to deal with these — for the most part, the lifetime should be `'static`
#[derive(Debug, Clone)]
pub enum SocketValue<'a> {
    /// No value; unconnected socket.
    None,

    /// One single event.
    IndividualEvent(Box<super::Event>),

    /// A collection of events that do not overlap. For each point in time, there will be
    /// at most one event in the collection that is visible at that time. Note that there may
    /// still be gaps between events.
    /// Most commonly this will be frame-by-frame events or the like.
    MonotonicEvents(Vec<super::Event>),

    /// An arbitrary collection of events, may overlap in time/space. For example rendered gradient
    /// fields or the like.
    GenericEvents(Vec<super::Event>),

    Sline(&'a subtitle::Sline),

    /// Compiled events that are ready to copy into libass.
    CompiledEvents(Vec<subtitle::ass::Event<'static>>),
}

impl<'a> SocketValue<'a> {
    pub fn map_events<F>(&self, callback: F) -> Result<SocketValue<'static>, NodeError>
    where
        F: Fn(&super::Event) -> super::Event,
    {
        match self {
            SocketValue::IndividualEvent(event) => Ok(SocketValue::IndividualEvent(Box::new(
                callback(event.as_ref()),
            ))),
            SocketValue::MonotonicEvents(events) => Ok(SocketValue::MonotonicEvents(
                events.iter().map(callback).collect(),
            )),
            SocketValue::GenericEvents(events) => Ok(SocketValue::GenericEvents(
                events.iter().map(callback).collect(),
            )),
            _ => Err(NodeError::MismatchedTypes),
        }
    }

    pub fn map_events_into<T, F>(&self, callback: F) -> Result<Vec<T>, NodeError>
    where
        F: Fn(&super::Event) -> T,
    {
        match self {
            SocketValue::IndividualEvent(event) => Ok(vec![callback(event.as_ref())]),
            SocketValue::MonotonicEvents(events) => Ok(events.iter().map(callback).collect()),
            SocketValue::GenericEvents(events) => Ok(events.iter().map(callback).collect()),
            _ => Err(NodeError::MismatchedTypes),
        }
    }
}

/// Represents a type of socket, as in, what kind of value a node would like to have passed into it.
#[derive(Debug)]
pub enum SocketType {
    IndividualEvent,
    MonotonicEvents,
    GenericEvents,

    /// This represents an “input” to a leaf node, i.e. a node that does not have a user-assignable
    /// input and thus acts as a node that supplies an initial value to the graph.
    LeafInput(LeafInputType),
}

impl SocketType {
    pub fn is_event(&self) -> bool {
        matches!(
            self,
            SocketType::IndividualEvent | SocketType::MonotonicEvents | SocketType::GenericEvents
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LeafInputType {
    Sline,
}

pub trait Node: Debug {
    fn name(&self) -> &'static str;
    fn desired_inputs(&self) -> &[SocketType];
    fn predicted_outputs(&self) -> &[SocketType];
    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, NodeError>;
}

pub enum NodeError {
    MismatchedTypes,
}
