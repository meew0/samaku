use std::borrow::Cow;

pub use graph::Graph;
pub use node::Node;

use crate::subtitle;

pub mod graph;
pub mod node;
pub mod tags;

#[derive(Debug)]
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
    pub fn to_ass_event(&self) -> subtitle::ass::Event<'static> {
        let mut compiled_text = String::new();

        // Reused buffer for compiled tags
        let mut compiled_tags = String::new();

        self.global_tags
            .emit(&mut compiled_tags)
            .expect("emitting tags into a String should not fail");
        maybe_write_block(&mut compiled_text, compiled_tags.as_str());

        for (i, element) in self.text.iter().enumerate() {
            match element {
                Span::Tags(tags, text) => {
                    let mut new_tags = tags.clone();

                    if i == 0 {
                        new_tags.override_from(&self.overrides, false);
                    } else {
                        new_tags.clear_from(&self.overrides);
                    }

                    compiled_tags.clear();
                    new_tags
                        .emit(&mut compiled_tags)
                        .expect("emitting tags into a String should not fail");
                    maybe_write_block(&mut compiled_text, compiled_tags.as_str());
                    compiled_text.push_str(text);
                }
                Span::Reset => todo!(),
                Span::ResetToStyle(_) => todo!(),
                Span::Drawing(_, _) => todo!(),
            }
        }

        subtitle::ass::Event {
            start: self.start,
            duration: self.duration,
            layer_index: self.layer_index,
            style_index: self.style_index,
            margins: self.margins,
            text: Cow::from(compiled_text),
            read_order: 0,
            name: Cow::from(""),
            effect: Cow::from(""),
        }
    }
}

fn maybe_write_block(text: &mut String, tags: &str) {
    if !tags.is_empty() {
        text.push('{');
        text.push_str(tags);
        text.push('}');
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

    /// Moves the local overrides out of this span, if present, and discards the rest.
    /// Guaranteed to return `Some(_)` if and only if this span is `Tags` or `Drawing`.
    fn into_local(self) -> Option<tags::Local> {
        match self {
            Self::Tags(local, _) => Some(local),
            Self::Drawing(local, _) => Some(local),
            _ => None,
        }
    }
}
