mod graph;
mod tags;

#[allow(clippy::large_enum_variant)]
enum Span {
    /// Some text tagged with override tags.
    Tags(tags::Local, String),

    /// Reset overrides to the default style.
    Reset,

    /// Reset overrides to a named style.
    ResetToStyle(String),

    /// Vector drawing
    Drawing(tags::Drawing),
}
