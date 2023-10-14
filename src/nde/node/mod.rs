use std::fmt::Debug;

pub use input::Position as InputPosition;
pub use input::Sline as InputSline;
pub use output::Output;
pub use style_basic::Italic;

use crate::{message, model, nde, subtitle};

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

    Position(nde::tags::Position),

    /// Compiled events that are ready to copy into libass.
    CompiledEvents(Vec<subtitle::ass::Event<'static>>),
}

impl<'a> SocketValue<'a> {
    #[must_use]
    pub fn as_type(&self) -> Option<SocketType> {
        match self {
            SocketValue::IndividualEvent(_) => Some(SocketType::IndividualEvent),
            SocketValue::MonotonicEvents(_) => Some(SocketType::MonotonicEvents),
            SocketValue::GenericEvents(_) => Some(SocketType::GenericEvents),
            SocketValue::Position(_) => Some(SocketType::Position),
            SocketValue::None | SocketValue::Sline(_) | SocketValue::CompiledEvents(_) => None,
        }
    }

    /// Assumes `self` contains events of some kind, and maps those events one-by-one using the
    /// given function, returning a [`SocketValue`] of the same kind as `self`.
    ///
    /// # Errors
    /// Returns [`Error::MismatchedTypes`] if `self` does not contain events.
    pub fn map_events<F>(&self, func: F) -> Result<SocketValue<'static>, Error>
    where
        F: Fn(&super::Event) -> super::Event,
    {
        match self {
            SocketValue::IndividualEvent(event) => {
                Ok(SocketValue::IndividualEvent(Box::new(func(event.as_ref()))))
            }
            SocketValue::MonotonicEvents(events) => Ok(SocketValue::MonotonicEvents(
                events.iter().map(func).collect(),
            )),
            SocketValue::GenericEvents(events) => Ok(SocketValue::GenericEvents(
                events.iter().map(func).collect(),
            )),
            _ => Err(Error::MismatchedTypes),
        }
    }

    /// Assumes `self` contains events of some kind, and maps those events one-by-one using the
    /// given function, returning a [`Vec`] of returned values.
    ///
    /// # Errors
    /// Returns [`Error::MismatchedTypes`] if `self` does not contain events.
    pub fn map_events_into<T, F>(&self, func: F) -> Result<Vec<T>, Error>
    where
        F: Fn(&super::Event) -> T,
    {
        match self {
            SocketValue::IndividualEvent(event) => Ok(vec![func(event.as_ref())]),
            SocketValue::MonotonicEvents(events) | SocketValue::GenericEvents(events) => {
                Ok(events.iter().map(func).collect())
            }
            _ => Err(Error::MismatchedTypes),
        }
    }
}

/// Represents a type of socket, as in, what kind of value a node would like to have passed into it.
#[derive(Debug, Clone, Copy)]
pub enum SocketType {
    IndividualEvent,
    MonotonicEvents,
    GenericEvents,
    Position,

    /// This represents an “input” to a leaf node, i.e. a node that does not have a user-assignable
    /// input and thus acts as a node that supplies an initial value to the graph.
    LeafInput(LeafInputType),
}

impl SocketType {
    #[must_use]
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

    /// Run the action defined by this node. `inputs` can be assumed to have the same length as
    /// [`desired_inputs`], but the specific types may be different. If it returns `Ok`, the `Vec`
    /// should have the same length as [`predicted_outputs`], but the specific types may again
    /// be different.
    ///
    /// # Errors
    /// Can return an [`Error`] to indicate that the node is unable to process the given inputs
    /// for whatever reason, for example due to mismatched input types.
    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error>;

    /// Content elements that should be displayed at the top of the node. By default, this is simply
    /// some text showing the node's name.
    fn content<'a>(
        &self,
        _self_index: usize,
    ) -> iced::Element<'a, message::Message, iced::Renderer> {
        iced::widget::text(self.name()).into()
    }

    /// Called when a reticule claiming to originate from this node is updated.
    fn reticule_update(&mut self, _reticules: &[model::reticule::Reticule]) {}

    /// The node's content size, used for layouting the content.
    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 75.0)
    }
}

pub enum Error {
    MismatchedTypes,
}

/// An “empty shell” representation of the operation performed by a node. Used to represent the
/// idea of a node e.g. in the `AddNode` message.
#[derive(Debug, Clone)]
pub enum Shell {
    InputSline,
    InputPosition,
    Italic,
}

impl Shell {
    #[must_use]
    pub fn instantiate(&self) -> Box<dyn Node> {
        match self {
            Shell::InputSline => Box::new(InputSline {}),
            Shell::InputPosition => Box::new(InputPosition {
                value: nde::tags::Position { x: 0.0, y: 0.0 },
            }),
            Shell::Italic => Box::new(Italic {}),
        }
    }
}
