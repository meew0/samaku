use crate::subtitle;

use super::tags;

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

    Sline(&'a subtitle::Sline<'a>),

    /// Compiled events that are ready to copy into libass.
    CompiledEvents(Vec<subtitle::ass::Event<'static>>),
}

impl<'a> SocketValue<'a> {
    pub fn map_events<F>(&self, callback: F) -> SocketValue<'static>
    where
        F: Fn(&super::Event) -> super::Event,
    {
        match self {
            SocketValue::IndividualEvent(event) => {
                SocketValue::IndividualEvent(Box::new(callback(event.as_ref())))
            }
            SocketValue::MonotonicEvents(events) => {
                SocketValue::MonotonicEvents(events.iter().map(callback).collect())
            }
            SocketValue::GenericEvents(events) => {
                SocketValue::GenericEvents(events.iter().map(callback).collect())
            }
            _ => panic!("expected events"),
        }
    }

    pub fn map_events_into<T, F>(&self, callback: F) -> Vec<T>
    where
        F: Fn(&super::Event) -> T,
    {
        match self {
            SocketValue::IndividualEvent(event) => {
                vec![callback(event.as_ref())]
            }
            SocketValue::MonotonicEvents(events) => events.iter().map(callback).collect(),
            SocketValue::GenericEvents(events) => events.iter().map(callback).collect(),
            _ => panic!("expected events"),
        }
    }
}

/// Represents a type of socket, as in, what kind of value a node would like to have passed into it.
#[derive(Debug, Clone, Copy)]
pub enum SocketType {
    IndividualEvent,
    MonotonicEvents,
    GenericEvents,

    /// This represents an “input” to a leaf node, i.e. a node that does not have a user-assignable
    /// input and thus acts as a node that supplies an initial value to the graph.
    LeafInput(LeafInputType),
}

#[derive(Debug, Clone, Copy)]
pub enum LeafInputType {
    Sline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Output,
    InputSline,
    Italic,
    TestNodeWithTwoInputs,
}

impl Node {
    pub fn desired_inputs(&self) -> &[SocketType] {
        match self {
            Node::Output => &[SocketType::GenericEvents],
            Node::InputSline => &[SocketType::LeafInput(LeafInputType::Sline)],
            Node::Italic => &[SocketType::GenericEvents],
            Node::TestNodeWithTwoInputs => {
                &[SocketType::IndividualEvent, SocketType::IndividualEvent]
            }
        }
    }

    pub fn run(&self, inputs: &[&SocketValue]) -> Vec<SocketValue> {
        match self {
            Node::Output => {
                let compiled = inputs[0].map_events_into(super::Event::to_ass_event);
                vec![SocketValue::CompiledEvents(compiled)]
            }
            Node::InputSline => {
                let sline = match inputs[0] {
                    SocketValue::Sline(sline) => sline,
                    _ => panic!("expected sline"),
                };
                let event = super::Event {
                    start: sline.start,
                    duration: sline.duration,
                    layer_index: sline.layer_index,
                    style_index: sline.style_index,
                    margins: sline.margins,
                    global_tags: tags::Global::empty(),
                    overrides: tags::Local::empty(),

                    // TODO in the far future: parse ASS tags into span?
                    text: vec![super::Span::Tags(tags::Local::empty(), sline.text.clone())],
                };
                vec![SocketValue::IndividualEvent(Box::new(event))]
            }
            Node::Italic => vec![inputs[0].map_events(|event| {
                let mut new_event = event.clone();
                new_event.overrides.italic = Some(true);
                new_event
            })],
            Node::TestNodeWithTwoInputs => todo!(),
        }
    }
}
