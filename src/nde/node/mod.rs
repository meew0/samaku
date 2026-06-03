#![allow(
    clippy::module_name_repetitions,
    reason = "`typetag` for (de)serialising trait objects requires that every struct that implements `Node` must have a unique name. So even if it would lead to more concise code, we can't have the individual node structs be named e.g. `clip::Rectangle` and `input::Rectangle`."
)]

use std::fmt::Debug;

pub use clip::ClipRectangle;
pub use gradient::Gradient;
pub use input::InputEvent;
pub use input::InputPosition;
pub use input::InputQuad;
pub use input::InputRectangle;
pub use input::InputTags;
pub use motion_track::MotionTrack;
pub use output::Output;
pub use perspective::Perspective;
pub use positioning::SetPosition;
pub use split::SplitFrameByFrame;
pub use style_basic::Italic;

use crate::{message, model, nde, subtitle};

mod clip;
mod gradient;
mod input;
mod motion_track;
mod output;
mod perspective;
mod positioning;
mod split;
mod style_basic;

/// Represents a value passed into a socket.
///
/// This struct takes a lifetime argument because it needs to support references to events stored
/// in the global state. But those should not be passed around in the node tree, so only leaf
/// nodes should have to deal with these — for the most part, the lifetime should be `'static`.
#[derive(Debug, Clone)]
pub enum SocketValue<'a> {
    /// No value; unconnected socket.
    None,

    /// One single event.
    IndividualEvent(Box<super::Event>),

    /// A collection of events. Can have any length.
    MultipleEvents(Vec<super::Event>),

    LocalTags(Box<super::tags::Local>),
    GlobalTags(Box<super::tags::Global>),

    Position(glam::DVec2),
    Rectangle(nde::tags::Rectangle),
    Quad(nde::tags::perspective::Quad),

    SourceEvent(&'a subtitle::Event<'static>),

    /// Compiled events that are ready to copy into libass.
    CompiledEvents(Vec<subtitle::Event<'static>>),
}

impl SocketValue<'_> {
    #[must_use]
    pub fn as_type(&self) -> Option<SocketType> {
        match *self {
            SocketValue::IndividualEvent(_) => Some(SocketType::IndividualEvent),
            SocketValue::MultipleEvents(_) => Some(SocketType::MultipleEvents),
            SocketValue::LocalTags(_) => Some(SocketType::LocalTags),
            SocketValue::GlobalTags(_) => Some(SocketType::GlobalTags),
            SocketValue::Position(_) => Some(SocketType::Position),
            SocketValue::Rectangle(_) => Some(SocketType::Rectangle),
            SocketValue::Quad(_) => Some(SocketType::Quad),
            SocketValue::None | SocketValue::SourceEvent(_) | SocketValue::CompiledEvents(_) => {
                None
            }
        }
    }

    #[must_use]
    pub fn identifier(&self) -> &'static str {
        match *self {
            SocketValue::None => "None",
            SocketValue::IndividualEvent(_) => "IndividualEvent",
            SocketValue::MultipleEvents(_) => "MultipleEvents",
            SocketValue::LocalTags(_) => "LocalTags",
            SocketValue::GlobalTags(_) => "GlobalTags",
            SocketValue::Position(_) => "Position",
            SocketValue::Rectangle(_) => "Rectangle",
            SocketValue::Quad(_) => "Quad",
            SocketValue::SourceEvent(_) => "SourceEvent",
            SocketValue::CompiledEvents(_) => "CompiledEvents",
        }
    }

    /// Assumes `self` contains events of some kind, and maps those events one-by-one using the
    /// given function, returning a [`SocketValue`] of the same kind as `self`.
    ///
    /// # Errors
    /// Returns [`BasicError::MismatchedTypes`] if `self` does not contain events.
    pub fn map_events<F>(&self, func: F) -> anyhow::Result<SocketValue<'static>>
    where
        F: Fn(&super::Event) -> super::Event,
    {
        match *self {
            SocketValue::IndividualEvent(ref event) => {
                Ok(SocketValue::IndividualEvent(Box::new(func(event.as_ref()))))
            }
            SocketValue::MultipleEvents(ref events) => Ok(SocketValue::MultipleEvents(
                events.iter().map(func).collect(),
            )),
            ref other => type_error(other, "event(s)"),
        }
    }

    /// Assumes `self` contains events of some kind, and maps those events one-by-one using the
    /// given function, returning a [`Vec`] of returned values.
    ///
    /// # Errors
    /// Returns [`BasicError::MismatchedTypes`] if `self` does not contain events.
    pub fn map_events_into<T, F>(&self, func: F) -> anyhow::Result<Vec<T>>
    where
        F: Fn(&super::Event) -> T,
    {
        match *self {
            SocketValue::IndividualEvent(ref event) => Ok(vec![func(event.as_ref())]),
            SocketValue::MultipleEvents(ref events) => Ok(events.iter().map(func).collect()),
            ref other => type_error(other, "event(s)"),
        }
    }

    /// Assumes `self` contains events of some kind, and calls the given mutable closure for each
    /// given event.
    ///
    /// # Errors
    /// Returns [`BasicError::MismatchedTypes`] if `self` does not contain events.
    pub fn each_event<F>(&self, mut func: F) -> anyhow::Result<()>
    where
        F: FnMut(&super::Event),
    {
        match *self {
            SocketValue::IndividualEvent(ref event) => func(event.as_ref()),
            SocketValue::MultipleEvents(ref events) => {
                for event in events {
                    func(event);
                }
            }
            ref other => return type_error(other, "event(s)"),
        }
        Ok(())
    }
}

// Handle an undesired socket value: if it is None, return a missing input error,
// if it is any other type, report mismatched types.
pub fn type_error<T>(other: &SocketValue, expected: &'static str) -> anyhow::Result<T> {
    if matches!(other, SocketValue::None) {
        Err(BasicError::MissingInput.into())
    } else {
        // slightly hacky
        let reason = if let Some(stripped) = expected.strip_prefix("SocketValue::") {
            let sub = match stripped.split_once('(') {
                Some((sub, _)) => sub,
                None => stripped,
            };
            anyhow::anyhow!("expected {sub}, got {}", other.identifier())
        } else {
            anyhow::anyhow!("expected {expected}, got {}", other.identifier())
        };
        Err(reason.context(BasicError::MismatchedTypes))
    }
}

macro_rules! retrieve {
    ($val:expr, $pattern:pat) => {
        let value = $val;
        let $pattern = value else {
            return crate::nde::node::type_error(&value, stringify!($pattern));
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
    LocalTags,
    GlobalTags,
    Position,
    Rectangle,
    Quad,
}

impl SocketType {
    #[must_use]
    pub fn is_event(self) -> bool {
        matches!(
            self,
            SocketType::IndividualEvent | SocketType::MultipleEvents | SocketType::AnyEvents
        )
    }
}

pub type Context<'a> = subtitle::compile::Context<'a>;

#[typetag::serde(tag = "type")]
#[expect(unused_variables, reason = "trait with default methods")]
pub trait Node: dyn_clone::DynClone + Debug + Send {
    fn name(&self) -> &'static str;

    /// The logical place of this node in the graph (input, process/intermediate, or output).
    ///
    /// Affects primarily the header color, but it may also be ascribed logical value,
    /// for instance when checking that an output node is present in the graph.
    fn category(&self) -> Category {
        Category::Process
    }

    fn desired_inputs(&self) -> &[SocketType];
    fn predicted_outputs(&self) -> &[SocketType];

    /// Run the action defined by this node. `inputs` can be assumed to have the same length as
    /// [`desired_inputs`], but the specific types may be different. If it returns `Ok`, the `Vec`
    /// should have the same length as [`predicted_outputs`], but the specific types may again
    /// be different.
    ///
    /// # Errors
    /// Can return an [`BasicError`] to indicate that the node is unable to process the given inputs
    /// for whatever reason, for example due to mismatched input types.
    fn run(
        &'_ self,
        inputs: &[&SocketValue],
        context: &Context,
    ) -> anyhow::Result<Vec<SocketValue<'_>>>;

    /// Content elements that should be displayed at the top of the node. By default, this is simply
    /// some text showing the node's name.
    fn content<'a>(
        &'a self,
        global_state: &'a crate::Samaku,
        filter_index: subtitle::ExtradataId,
        self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        iced::widget::space().into()
    }

    /// Called when a message for this node is received.
    fn update(&mut self, message: message::Node) -> anyhow::Result<()> {
        anyhow::bail!("Node '{}' does not know how to handle message", self.name());
    }

    /// This method is called if only this node is selected. If desired, the node can return `Some`
    /// reticules in order to activate them, such that they are displayed above the video. If they
    /// are moved, `reticule_update` will later be called.
    ///
    /// The `context` is available in order to calculate reticule positions based on event
    /// properties (such as the text bounding box). This should be seen more as a convenience
    /// feature than anything that should be relied on for correctness; in general, node behavior
    /// should be independent of the active event (only depend on the event the node is run on).
    fn reticule_activate(
        &mut self,
        context: &subtitle::compile::Context,
    ) -> Vec<model::reticule::Reticule> {
        vec![]
    }

    /// Called when a reticule claiming to originate from this node is moved. The node takes care
    /// of actually updating the data — it can introduce complex logic here to link some reticules
    /// to others etc.
    /// For undo purposes, this should return the former position. Also, the logic should be reversible,
    /// such that if afterwards, `reticule_update` is called again with the previous position, it should
    /// put the node in the same state as it would have been before.
    fn reticule_update(
        &mut self,
        reticules: &mut model::reticule::Reticules,
        index: model::reticule::Index,
        new_position: glam::DVec2,
    ) -> anyhow::Result<glam::DVec2> {
        anyhow::bail!("Node '{}' does not provide any reticules", self.name());
    }

    /// This method is called before any reticule is drawn.
    /// The node can draw something as a base layer to be shown below the reticules.
    fn draw_reticule_base_layer(
        &self,
        canvas_frame: &mut iced::widget::canvas::Frame,
        bounds: iced::Rectangle,
        storage_size: subtitle::Resolution,
        current_frame: Option<model::FrameNumber>,
        cursor: iced::mouse::Cursor,
    ) {
    }

    /// The node's content size, used for layouting the content.
    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 75.0)
    }
}

// Generates an impl of `Clone` for `Box<dyn Node>`, to ensure nodes (and thus NDE filters) are cloneable
dyn_clone::clone_trait_object!(Node);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Input,
    Process,
    Output,
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum BasicError {
    #[error("Mismatched types")]
    MismatchedTypes,

    #[error("Missing required input")]
    MissingInput,
}

pub type Constructor = fn() -> Box<dyn Node>;

/// An empty “shell” of a node that can be used to initiate a node later on.
///
/// Represents the idea of a node, with any specific type information being erased. We use the
/// `inventory` crate to collect node shells, to be able to iterate over them in all places where
/// we need a list of registered nodes (like in the node editor “Add” menu).
pub struct Shell {
    pub menu_path: &'static [&'static str],
    pub constructor: Constructor,
}

impl Shell {
    pub const fn new(menu_path: &'static [&'static str], constructor: Constructor) -> Self {
        Self {
            menu_path,
            constructor,
        }
    }
}

inventory::collect!(Shell);
