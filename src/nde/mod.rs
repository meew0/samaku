use std::borrow::Cow;

pub use graph::Graph;
pub use node::Node;

use crate::subtitle;

pub mod graph;
pub mod node;
pub mod tags;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Filter {
    pub name: String,
    pub graph: Graph,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub start: subtitle::StartTime,
    pub duration: subtitle::Duration,
    pub layer_index: i32,
    pub style_index: i32,
    pub margins: subtitle::Margins,

    /// Tags applying to the entire line.
    pub global_tags: tags::Global,

    /// Global overrides for local tags: normally these tags would only apply to a specific section
    /// of text and could be overridden by future occurrencies. But if one of the tags in this field
    /// is set, it will be removed from all local tag sets in compilation, such that it is
    /// guaranteed to apply over the entire event.
    pub overrides: tags::Local,

    /// Text mixed with local tags, defining its style.
    pub text: Vec<Span>,
}

impl Event {
    #[must_use]
    pub fn to_ass_event(&self) -> subtitle::Event<'static> {
        let mut cloned_spans: Vec<Span> = vec![];

        for (i, element) in self.text.iter().enumerate() {
            let new_span = match element {
                Span::Tags(tags, text) => {
                    let new_tags = self.clone_and_maybe_override_or_clear(tags, i);
                    Span::Tags(new_tags, text.clone())
                }
                Span::Reset => Span::Reset,
                Span::ResetToStyle(style_name) => Span::ResetToStyle(style_name.clone()),
                Span::Drawing(tags, drawing) => {
                    let new_tags = self.clone_and_maybe_override_or_clear(tags, i);
                    Span::Drawing(new_tags, drawing.clone())
                }
            };

            cloned_spans.push(new_span);
        }

        let compiled_text = tags::emit(&self.global_tags, &cloned_spans);

        subtitle::Event {
            start: self.start,
            duration: self.duration,
            layer_index: self.layer_index,
            style_index: self.style_index,
            margins: self.margins,
            text: Cow::from(compiled_text),
            actor: Cow::from(""),
            effect: Cow::from(""),
            event_type: subtitle::EventType::Dialogue,
            extradata_ids: vec![],
        }
    }

    #[must_use]
    pub fn make_static(
        &self,
        time_point: subtitle::StartTime,
        static_duration: subtitle::Duration,
    ) -> Event {
        // TODO: take care of animations and the like
        Event {
            start: time_point,
            duration: static_duration,

            layer_index: self.layer_index,
            style_index: self.style_index,
            margins: self.margins,
            global_tags: self.global_tags.clone(),
            overrides: self.overrides.clone(),
            text: self.text.clone(),
        }
    }

    fn clone_and_maybe_override_or_clear(&self, tags: &tags::Local, i: usize) -> tags::Local {
        let mut new_tags = tags.clone();

        if i == 0 {
            new_tags.override_from(&self.overrides, false);
        } else {
            new_tags.clear_from(&self.overrides);
        }

        new_tags
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum Span {
    /// Some text tagged with override tags.
    Tags(tags::Local, String),

    /// Reset overrides to the default style.
    Reset,

    /// Reset overrides to a named style.
    ResetToStyle(String),

    /// Vector drawing
    Drawing(tags::Local, tags::Drawing),
}

impl Span {
    /// Returns `true` if this span has no content (`content_is_empty` returns true) **and** the
    /// local tags are also empty. Returns `false` for `Reset` or `ResetToStyle`.
    fn is_empty(&self) -> bool {
        match self {
            Self::Tags(local, text) if text.is_empty() && *local == tags::Local::empty() => false,
            Self::Drawing(local, drawing)
                if drawing.is_empty() && *local == tags::Local::empty() =>
            {
                true
            }
            _ => false,
        }
    }

    /// Returns `true` if this `Span` has no content (`Tags`/`Drawing` with empty text/drawing,
    /// or either of the `Reset` variants) and `false` otherwise (`Tags`/`Drawing` with non-empty
    /// text/drawing).
    fn content_is_empty(&self) -> bool {
        match self {
            Self::Tags(_, text) if !text.is_empty() => false,
            Self::Drawing(_, drawing) if !drawing.is_empty() => false,
            _ => true,
        }
    }

    /// Returns `true` if this `Span` is `Reset` or `ResetToStyle`.
    fn is_reset(&self) -> bool {
        matches!(self, Self::Reset | Self::ResetToStyle(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde() {
        let mut graph = Graph::from_single_intermediate(Box::new(node::ClipRectangle {}));
        graph.nodes.push(graph::VisualNode {
            node: Box::new(node::InputRectangle {
                value: tags::Rectangle {
                    x1: 100,
                    y1: 200,
                    x2: 300,
                    y2: 400,
                },
            }),
            position: iced::Point::new(0.0, 400.0),
        });

        graph.connections.insert(
            graph::NextEndpoint {
                node_index: 1,
                socket_index: 1,
            },
            graph::PreviousEndpoint {
                node_index: 3,
                socket_index: 0,
            },
        );

        let filter = Filter {
            graph,
            name: "test filter".to_string(),
        };

        let mut data: Vec<u8> = vec![];
        ciborium::into_writer(&filter, &mut data).unwrap();
        println!("serialised filter: {data:02x?}");

        let b64 = data_encoding::BASE64.encode(data.as_slice());
        println!("serialised filter b64: len {} data {}", b64.len(), b64);

        for level in 0..=10 {
            let b64z = data_encoding::BASE64
                .encode(miniz_oxide::deflate::compress_to_vec(data.as_slice(), level).as_slice());
            println!(
                "serialised filter compressed (level {level}): len {} data {}",
                b64z.len(),
                b64z
            );
        }

        let deserialised_filter = ciborium::from_reader::<Filter, _>(data.as_slice()).unwrap();
        assert_eq!(deserialised_filter.graph.nodes.len(), 4);
        assert_eq!(
            deserialised_filter.graph.nodes[3].position,
            iced::Point::new(0.0, 400.0)
        );
    }
}
