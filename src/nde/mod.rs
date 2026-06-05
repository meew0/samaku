use std::borrow::Cow;

use crate::nde::tags::{FontSize, FontWeight, Resettable};
use crate::{media, model, subtitle};
pub use graph::Graph;
pub use node::Node;

pub mod graph;
pub mod node;
pub mod tags;
pub mod util;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Filter {
    pub name: String,
    pub graph: Graph,
}

impl model::Named for Filter {
    fn name(&self) -> &str {
        if self.name.is_empty() {
            "(unnamed filter)"
        } else {
            &self.name
        }
    }
}

/// An NDE event. Differs from [`subtitle::Event`] in that the event text is represented
/// in parsed form, i.e. as global tags and a vector of tag/content spans.
#[derive(Debug, Clone)]
pub struct Event {
    pub start: model::FrameNumber,
    pub duration: model::FrameDelta,
    pub layer_index: i32,
    pub style_index: usize,
    pub margins: subtitle::Margins,

    /// Tags applying to the entire line.
    pub global_tags: tags::Global,

    /// Global overrides for local tags: normally these tags would only apply to a specific section
    /// of text and could be overridden by future occurrences. But if one of the tags in this field
    /// is set, it will be removed from all local tag sets in compilation, such that it is
    /// guaranteed to apply over the entire event.
    pub overrides: tags::Local,

    /// Text mixed with local tags, defining its style.
    pub text: Vec<Span>,
}

impl Event {
    #[must_use]
    pub fn from_ass_event(ass_event: &subtitle::Event, frame_rate: media::FrameRate) -> Self {
        let (global, spans) = tags::parse(&ass_event.text);

        let start_frame = frame_rate.ass_time_to_frame(ass_event.start);
        let end_frame = frame_rate.ass_time_to_frame_after(ass_event.start + ass_event.duration);
        let frame_count = end_frame - start_frame;

        Self {
            start: start_frame,
            duration: frame_count,
            layer_index: ass_event.layer_index,
            style_index: ass_event.style_index,
            margins: ass_event.margins,
            global_tags: *global,
            overrides: tags::Local::empty(),
            text: spans,
        }
    }

    #[must_use]
    pub fn to_ass_event(&self, frame_rate: media::FrameRate) -> subtitle::Event<'static> {
        let mut cloned_spans: Vec<Span> = vec![];

        for (i, element) in self.text.iter().enumerate() {
            let new_span = match *element {
                Span::Tags(ref tags, ref text) => {
                    let new_tags = self.clone_and_maybe_override_or_clear(tags, i);
                    Span::Tags(new_tags, text.clone())
                }
                Span::Reset => Span::Reset,
                Span::ResetToStyle(ref style_name) => Span::ResetToStyle(style_name.clone()),
                Span::Drawing(ref tags, ref drawing) => {
                    let new_tags = self.clone_and_maybe_override_or_clear(tags, i);
                    Span::Drawing(new_tags, drawing.clone())
                }
            };

            cloned_spans.push(new_span);
        }

        let compiled_text = tags::emit(&self.global_tags, &cloned_spans);

        let start = frame_rate.frame_to_ass_time(self.start);
        let duration = frame_rate.frame_to_ass_time(self.start + self.duration) - start;

        subtitle::Event {
            start,
            duration,
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
    pub fn make_static(&self, start: model::FrameNumber, duration: model::FrameDelta) -> Event {
        // TODO: take care of animations and the like
        Event {
            start,
            duration,
            layer_index: self.layer_index,
            style_index: self.style_index,
            margins: self.margins,
            global_tags: self.global_tags.clone(),
            overrides: self.overrides.clone(),
            text: self.text.clone(),
        }
    }

    #[must_use]
    pub fn first_local(&self) -> Option<&tags::Local> {
        if let Some(&Span::Tags(ref local, _) | &Span::Drawing(ref local, _)) = self.text.first() {
            Some(local)
        } else {
            None
        }
    }

    fn effective_tag<'a, T>(
        &'a self,
        tag_fn: for<'b> fn(&'b tags::Local) -> &'b Resettable<T>,
        style_value: &'a T,
    ) -> &'a T {
        match *tag_fn(&self.overrides) {
            Resettable::Override(ref x) => x,
            Resettable::Reset => style_value,
            Resettable::Keep => {
                if let Some(first_local) = self.first_local() {
                    tag_fn(first_local).override_or(style_value)
                } else {
                    style_value
                }
            }
        }
    }

    #[must_use]
    pub fn effective_border(&self, style: &subtitle::Style) -> glam::DVec2 {
        let x = *self.effective_tag(|local| &local.border.x, &style.border_width);
        let y = *self.effective_tag(|local| &local.border.y, &style.border_width);
        glam::DVec2::new(x, y)
    }

    #[must_use]
    pub fn effective_shadow(&self, style: &subtitle::Style) -> glam::DVec2 {
        let x = *self.effective_tag(|local| &local.shadow.x, &style.shadow_distance);
        let y = *self.effective_tag(|local| &local.shadow.y, &style.shadow_distance);
        glam::DVec2::new(x, y)
    }

    #[must_use]
    pub fn effective_font_scale(&self, style: &subtitle::Style) -> glam::DVec2 {
        let x = *self.effective_tag(|local| &local.font_scale.x, &style.scale.x);
        let y = *self.effective_tag(|local| &local.font_scale.y, &style.scale.y);
        glam::DVec2::new(x, y)
    }

    #[must_use]
    pub fn effective_font_name<'a>(&'a self, style: &'a subtitle::Style) -> &'a str {
        let str: &String = self.effective_tag(|local| &local.font_name, &style.font_name);
        str.as_str()
    }

    #[must_use]
    pub fn effective_font_size(&self, style: &subtitle::Style) -> f64 {
        let val = match self.overrides.font_size {
            FontSize::Set(val) => val,
            FontSize::Reset(delta) => delta.apply(style.font_size),
            FontSize::Delta(delta) => {
                if let Some(first_local) = self.first_local() {
                    delta.apply(match first_local.font_size {
                        FontSize::Set(val) => val,
                        FontSize::Reset(inner_delta) | FontSize::Delta(inner_delta) => {
                            inner_delta.apply(style.font_size)
                        }
                    })
                } else {
                    delta.apply(style.font_size)
                }
            }
        };

        if val > 0.0 { val } else { style.font_size }
    }

    #[must_use]
    pub fn effective_font_weight(&self, style: &subtitle::Style) -> FontWeight {
        *self.effective_tag(
            |local| &local.font_weight,
            &FontWeight::BoldToggle(style.bold),
        )
    }

    #[must_use]
    pub fn effective_italic(&self, style: &subtitle::Style) -> bool {
        *self.effective_tag(|local| &local.italic, &style.italic)
    }

    #[must_use]
    pub fn effective_letter_spacing(&self, style: &subtitle::Style) -> f64 {
        *self.effective_tag(|local| &local.letter_spacing, &style.spacing)
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

#[derive(Debug, Clone)]
pub enum Span {
    /// Some text tagged with override tags.
    Tags(tags::Local, String),

    /// Reset overrides to the default style.
    Reset,

    /// Reset overrides to a named style.
    ResetToStyle(String),

    /// Vector drawing.
    Drawing(tags::Local, tags::Drawing),
}

impl Span {
    /// Returns `true` if this span has no content (`content_is_empty` returns true) **and** the
    /// local tags are also empty. Returns `false` for `Reset` or `ResetToStyle`.
    fn is_empty(&self) -> bool {
        match *self {
            Self::Tags(ref local, ref text)
                if text.is_empty() && *local == tags::Local::empty() =>
            {
                false
            }
            Self::Drawing(ref local, ref drawing)
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
        match *self {
            Self::Tags(_, ref text) if !text.is_empty() => false,
            Self::Drawing(_, ref drawing) if !drawing.is_empty() => false,
            _ => true,
        }
    }

    /// Returns `true` if this `Span` is `Reset` or `ResetToStyle`.
    fn is_reset(&self) -> bool {
        matches!(self, Self::Reset | Self::ResetToStyle(_))
    }
}

/// Represents the screen-space bounding box of an event.
#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    top_left: glam::DVec2,
    bottom_right: glam::DVec2,
}

impl BoundingBox {
    fn size(&self) -> glam::DVec2 {
        self.bottom_right - self.top_left
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
                node_index: graph::NodeId(1),
                socket_index: graph::SocketId(1),
            },
            graph::PreviousEndpoint {
                node_index: graph::NodeId(3),
                socket_index: graph::SocketId(0),
            },
        );

        let filter = Filter {
            graph,
            name: "test filter".to_owned(),
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
