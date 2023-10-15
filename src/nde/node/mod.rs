use std::collections::HashMap;
use std::fmt::Debug;

pub use input::FrameRate as InputFrameRate;
pub use input::Position as InputPosition;
pub use input::Sline as InputSline;
pub use motion_track::MotionTrack;
pub use output::Output;
pub use positioning::SetPosition;
pub use split::FrameByFrame as SplitFrameByFrame;
pub use style_basic::Italic;

use crate::{media, message, model, nde, subtitle};

mod input;
mod motion_track;
mod output;
mod positioning;
mod split;
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

    /// A collection of events. Can have any length.
    MultipleEvents(Vec<super::Event>),

    Position(nde::tags::Position),

    FrameRate(media::FrameRate),

    Sline(&'a subtitle::Sline),

    /// Compiled events that are ready to copy into libass.
    CompiledEvents(Vec<subtitle::ass::Event<'static>>),
}

impl<'a> SocketValue<'a> {
    #[must_use]
    pub fn as_type(&self) -> Option<SocketType> {
        match self {
            SocketValue::IndividualEvent(_) => Some(SocketType::IndividualEvent),
            SocketValue::MultipleEvents(_) => Some(SocketType::MultipleEvents),
            SocketValue::Position(_) => Some(SocketType::Position),
            SocketValue::FrameRate(_) => Some(SocketType::FrameRate),
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
            SocketValue::MultipleEvents(events) => Ok(SocketValue::MultipleEvents(
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
            SocketValue::MultipleEvents(events) => Ok(events.iter().map(func).collect()),
            _ => Err(Error::MismatchedTypes),
        }
    }
}

macro_rules! retrieve {
    ($val:expr, $pattern:pat) => {
        let value = $val;
        let $pattern = value else {
            return Err(Error::MismatchedTypes);
        };
    };
}

pub(crate) use retrieve;

/// Represents a type of socket, as in, what kind of value a node would like to have passed into it.
#[derive(Debug, Clone, Copy)]
pub enum SocketType {
    IndividualEvent,
    MultipleEvents,
    AnyEvents,
    Position,
    FrameRate,

    /// This represents an “input” to a leaf node, i.e. a node that does not have a user-assignable
    /// input and thus acts as a node that supplies an initial value to the graph.
    LeafInput(LeafInputType),
}

impl SocketType {
    #[must_use]
    pub fn is_event(&self) -> bool {
        matches!(
            self,
            SocketType::IndividualEvent | SocketType::MultipleEvents | SocketType::AnyEvents
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LeafInputType {
    Sline,
    FrameRate,
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
    #[allow(unused_variables)]
    fn content<'a>(
        &self,
        self_index: usize,
    ) -> iced::Element<'a, message::Message, iced::Renderer> {
        iced::widget::text(self.name()).into()
    }
    
    /// Called when a message for this node is received.
    #[allow(unused_variables)]
    fn update(&mut self, message: message::Node) {
    }
    
    /// Called when a reticule claiming to originate from this node is moved. The node takes care
    /// of actually updating the data — it can introduce complex logic here to link some reticules
    /// to others etc.
    #[allow(unused_variables)]
    fn reticule_update(
        &mut self,
        reticules: &mut model::reticule::Reticules,
        index: usize,
        new_position: nde::tags::Position,
    ) {
    }

    /// The node's content size, used for layouting the content.
    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 75.0)
    }
}

pub enum Error {
    MismatchedTypes,
    Empty,
}

/// An “empty shell” representation of the operation performed by a node. Used to represent the
/// idea of a node e.g. in the `AddNode` message.
#[derive(Debug, Clone)]
pub enum Shell {
    InputSline,
    InputPosition,
    InputFrameRate,
    Italic,
    SetPosition,
    MotionTrack,
    SplitFrameByFrame,
}

impl Shell {
    #[must_use]
    pub fn instantiate(&self) -> Box<dyn Node> {
        match self {
            Shell::InputSline => Box::new(InputSline {}),
            Shell::InputPosition => Box::new(InputPosition {
                value: nde::tags::Position { x: 0.0, y: 0.0 },
            }),
            Shell::InputFrameRate => Box::new(InputFrameRate {}),
            Shell::Italic => Box::new(Italic {}),
            Shell::SetPosition => Box::new(SetPosition {}),
            Shell::MotionTrack => Box::new(MotionTrack {
                region_center: nde::tags::Position { x: 100.0, y: 100.0 },
                track: HashMap::new(),
            }),
            Shell::SplitFrameByFrame => Box::new(SplitFrameByFrame {}),
        }
    }
}
