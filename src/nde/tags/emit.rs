use crate::nde::Span;

/// Converts the given `spans` together with the given `global` tag overrides into a string of
/// ASS tag blocks, to be used by e.g. libass.
#[must_use]
#[allow(clippy::missing_panics_doc)] // the expectations should never fail
pub fn emit(global: &super::Global, spans: &[Span]) -> String {
    use std::fmt::Write;

    let mut compiled_text = String::new();

    // Reused buffer for compiled tags
    let mut compiled_tags = String::new();

    global
        .emit(&mut compiled_tags)
        .expect("emitting tags into a String should not fail");
    maybe_write_block(&mut compiled_text, compiled_tags.as_str());

    for element in spans {
        match element {
            Span::Tags(tags, text) => {
                compiled_tags.clear();
                tags.emit(&mut compiled_tags)
                    .expect("emitting tags into a String should not fail");
                maybe_write_block(&mut compiled_text, compiled_tags.as_str());
                push_escaped(&mut compiled_text, text);
            }
            Span::Reset => compiled_text.push_str("{\\r}"),
            Span::ResetToStyle(style_name) => {
                compiled_text.push_str("{\\r");
                compiled_text.push_str(style_name);
                compiled_text.push('}');
            }
            Span::Drawing(tags, drawing) => {
                compiled_tags.clear();
                tags.emit(&mut compiled_tags)
                    .expect("emitting tags into a String should not fail");
                maybe_write_block(&mut compiled_text, compiled_tags.as_str());
                write!(compiled_text, "{{\\p{}}}", drawing.scale)
                    .expect("writing drawing scale to String should not fail");
                push_escaped(&mut compiled_text, &drawing.commands);
                compiled_text.push_str("{\\p0}");
            }
        }
    }

    compiled_text
}

fn push_escaped(target: &mut String, source: &str) {
    for char in source.chars() {
        match char {
            '{' => target.push_str("\\{"),
            other => target.push(other),
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

pub trait TagName {
    fn write_name<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write;
}

pub trait Value {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write;
}

pub trait Tag {
    fn emit_tag<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write;
}

pub fn tag<W, T>(sink: &mut W, maybe_tag: &Option<T>) -> Result<(), std::fmt::Error>
where
    W: std::fmt::Write,
    T: Tag,
{
    if let Some(tag) = maybe_tag {
        tag.emit_tag(sink)?;
    }

    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
pub fn simple_tag<W, N, V>(
    sink: &mut W,
    tag_name: N,
    maybe_value: Option<&V>,
) -> Result<(), std::fmt::Error>
where
    W: std::fmt::Write,
    N: TagName,
    V: Value,
{
    if let Some(value) = maybe_value {
        sink.write_str("\\")?;
        tag_name.write_name(sink)?;
        value.emit_value(sink)?;
    }

    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
pub fn simple_tag_resettable<W, N, V>(
    sink: &mut W,
    tag_name: N,
    maybe_value: super::Resettable<&V>,
) -> Result<(), std::fmt::Error>
where
    W: std::fmt::Write,
    N: TagName,
    V: Value,
{
    if let super::Resettable::Keep = maybe_value {
    } else {
        sink.write_str("\\")?;
        tag_name.write_name(sink)?;
        if let super::Resettable::Override(value) = maybe_value {
            value.emit_value(sink)?;
        }
    }

    Ok(())
}

/// Behaves like `simple_tag`, but inserts parentheses around the argument.
pub fn complex_tag<W, V>(
    sink: &mut W,
    tag_name: &str,
    maybe_value: Option<&V>,
) -> Result<(), std::fmt::Error>
where
    W: std::fmt::Write,
    V: Value,
{
    if let Some(value) = maybe_value {
        sink.write_str("\\")?;
        sink.write_str(tag_name)?;
        sink.write_str("(")?;
        value.emit_value(sink)?;
        sink.write_str(")")?;
    }

    Ok(())
}

impl TagName for &str {
    fn write_name<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        sink.write_str(self)
    }
}

impl Value for bool {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        if *self {
            sink.write_str("1")?;
        } else {
            sink.write_str("0")?;
        }

        Ok(())
    }
}

macro_rules! emit_value_numeric {
    () => {
        fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
        where
            W: std::fmt::Write,
        {
            write!(sink, "{}", *self)
        }
    };
}

impl Value for f64 {
    emit_value_numeric!();
}

impl Value for u32 {
    emit_value_numeric!();
}

impl Value for i32 {
    emit_value_numeric!();
}

impl Value for String {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        sink.write_str(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() -> Result<(), std::fmt::Error> {
        let mut string = String::new();

        let no_value: Option<i32> = None;
        simple_tag(&mut string, "blub", no_value.as_ref())?;
        assert_eq!(string, "");

        Ok(())
    }

    #[test]
    fn some_values() -> Result<(), std::fmt::Error> {
        let mut string = String::new();

        let some_value: Option<i32> = Some(123);
        simple_tag(&mut string, "blub", some_value.as_ref())?;
        complex_tag(&mut string, "blubblub", some_value.as_ref())?;

        assert_eq!(string, "\\blub123\\blubblub(123)");

        Ok(())
    }
}
