#![allow(
    clippy::module_name_repetitions,
    reason = "`typetag` for (de)serialising trait objects requires that every struct that implements `Node` must have a unique name. So even if it would lead to more concise code, we can't have the individual node structs be named e.g. `clip::Rectangle` and `input::Rectangle`."
)]
#![allow(
    clippy::missing_asserts_for_indexing,
    reason = "false positives on retrieve! macro"
)]

use super::FramedTrack;
use std::fmt::Debug;

pub use clip::ClipRectangle;
pub use gradient::Gradient;
pub use input::InputEvent;
pub use input::InputMotionTrack;
pub use input::InputPosition;
pub use input::InputQuad;
pub use input::InputRectangle;
pub use input::InputTags;
pub use output::Output;
pub use perspective::Perspective;
pub use positioning::SetPosition;
pub use quad::MotionQuad;
pub use split::SplitFrameByFrame;
pub use style_basic::Italic;

use crate::{media, message, model, subtitle};

mod clip;
mod gradient;
mod input;
mod output;
mod perspective;
mod positioning;
mod quad;
mod split;
mod style_basic;

/// Represents a value passed into a socket.
///
/// The lifetime represents the lifetime of any data the contained value might borrow from:
/// for the most part, this is the lifetime of the `FramedTrack` frame adapter closure,
/// or also the `SourceEvent` lifetime.
#[derive(Debug, Clone, Default)]
pub enum SocketValue<'a> {
    /// No value; unconnected socket.
    #[default]
    None,

    Event(FramedTrack<'a, super::Event>),

    LocalTags(FramedTrack<'static, super::tags::Local>),
    GlobalTags(FramedTrack<'static, super::tags::Global>),

    Position(FramedTrack<'static, glam::DVec2>),
    Rectangle(FramedTrack<'static, super::tags::Rectangle>),
    Quad(FramedTrack<'static, super::tags::perspective::Quad>),
    Marker(FramedTrack<'static, media::motion::Marker>),

    /// Compiled events that are ready to copy into libass.
    CompiledEvents(Vec<subtitle::Event<'static>>),
}

impl<'a> SocketValue<'a> {
    #[must_use]
    pub fn as_type(&self) -> Option<SocketType> {
        match *self {
            SocketValue::Event(_) => Some(SocketType::Event),
            SocketValue::LocalTags(_) => Some(SocketType::LocalTags),
            SocketValue::GlobalTags(_) => Some(SocketType::GlobalTags),
            SocketValue::Position(_) => Some(SocketType::Position),
            SocketValue::Rectangle(_) => Some(SocketType::Rectangle),
            SocketValue::Quad(_) => Some(SocketType::Quad),
            SocketValue::Marker(_) => Some(SocketType::Marker),
            SocketValue::None | SocketValue::CompiledEvents(_) => None,
        }
    }

    #[must_use]
    pub fn identifier(&self) -> &'static str {
        match *self {
            SocketValue::None => "None",
            SocketValue::Event(_) => "Event",
            SocketValue::LocalTags(_) => "LocalTags",
            SocketValue::GlobalTags(_) => "GlobalTags",
            SocketValue::Position(_) => "Position",
            SocketValue::Rectangle(_) => "Rectangle",
            SocketValue::Quad(_) => "Quad",
            SocketValue::Marker(_) => "Marker",
            SocketValue::CompiledEvents(_) => "CompiledEvents",
        }
    }

    #[must_use]
    pub fn size(&self) -> Option<super::FramedTrackSize> {
        match *self {
            SocketValue::None | SocketValue::CompiledEvents(_) => None,
            SocketValue::Event(ref track) => Some(track.size()),
            SocketValue::LocalTags(ref track) => Some(track.size()),
            SocketValue::GlobalTags(ref track) => Some(track.size()),
            SocketValue::Position(ref track) => Some(track.size()),
            SocketValue::Rectangle(ref track) => Some(track.size()),
            SocketValue::Quad(ref track) => Some(track.size()),
            SocketValue::Marker(ref track) => Some(track.size()),
        }
    }

    #[must_use]
    pub fn into_values(self) -> SocketValues<'a> {
        smallvec::smallvec![self]
    }

    #[must_use]
    pub fn to_result(&self) -> SocketResult {
        SocketResult {
            value_type: self.as_type(),
            size: self.size(),
        }
    }
}

pub type SocketValues<'a> = smallvec::SmallVec<SocketValue<'a>, 2>;

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
        let value = std::mem::take($val);
        let $pattern = value else {
            return crate::nde::node::type_error(&value, stringify!($pattern));
        };
    };
}

pub(crate) use retrieve;

/// Converts a socket value containing either `Position` or `Marker` to a `Position` one.
pub fn marker_or_position<'a>(
    input: &mut SocketValue<'a>,
) -> anyhow::Result<FramedTrack<'a, glam::DVec2>> {
    match std::mem::take(input) {
        SocketValue::Position(position) => Ok(position),
        SocketValue::Marker(marker) => Ok(marker.map(|marker_val| marker_val.region.center)),
        other => anyhow::bail!("Expected position or marker, found {other:?}"),
    }
}

/// Represents a type of socket, as in, what kind of value a node would like to have passed into it.
#[derive(Debug, Clone, Copy)]
pub enum SocketType {
    Event,
    LocalTags,
    GlobalTags,
    Position,
    Rectangle,
    Quad,
    Marker,
}

impl SocketType {
    #[must_use]
    pub fn is_event(self) -> bool {
        matches!(self, SocketType::Event)
    }
}

pub struct SocketResult {
    pub value_type: Option<SocketType>,
    pub size: Option<super::FramedTrackSize>,
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
    fn run<'a>(
        &'_ self,
        inputs: SocketValues<'a>,
        context: &'a Context,
    ) -> anyhow::Result<SocketValues<'a>>;

    /// Content elements that should be displayed at the top of the node. By default, this is simply
    /// some text showing the node's name.
    fn content<'a>(
        &'a self,
        global_state: &'a crate::Samaku,
        filter_index: subtitle::ExtradataId,
        self_index: super::graph::NodeId,
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
        current_frame: Option<media::FrameNumber>,
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
