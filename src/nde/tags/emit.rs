use crate::nde::Span;
use crate::subtitle;

pub fn emit(global: &super::Global, spans: &[Span]) -> String {
    use std::fmt::Write;

    let mut compiled_text = String::new();

    // Reused buffer for compiled tags
    let mut compiled_tags = String::new();

    global
        .emit(&mut compiled_tags)
        .expect("emitting tags into a String should not fail");
    maybe_write_block(&mut compiled_text, compiled_tags.as_str());

    for element in spans.iter() {
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

pub trait EmitValue {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write;
}

pub trait EmitTag {
    fn emit_tag<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write;
}

pub fn tag<W, T>(sink: &mut W, maybe_tag: &Option<T>) -> Result<(), std::fmt::Error>
where
    W: std::fmt::Write,
    T: EmitTag,
{
    if let Some(tag) = maybe_tag {
        tag.emit_tag(sink)?;
    }

    Ok(())
}

pub fn simple_tag<W, N, V>(
    sink: &mut W,
    tag_name: N,
    maybe_value: Option<&V>,
) -> Result<(), std::fmt::Error>
where
    W: std::fmt::Write,
    N: TagName,
    V: EmitValue,
{
    if let Some(value) = maybe_value {
        sink.write_str("\\")?;
        tag_name.write_name(sink)?;
        value.emit_value(sink)?;
    }

    Ok(())
}

pub fn simple_tag_resettable<W, N, V>(
    sink: &mut W,
    tag_name: N,
    maybe_value: super::Resettable<&V>,
) -> Result<(), std::fmt::Error>
where
    W: std::fmt::Write,
    N: TagName,
    V: EmitValue,
{
    match maybe_value {
        super::Resettable::Keep => {}
        _ => {
            sink.write_str("\\")?;
            tag_name.write_name(sink)?;
            if let super::Resettable::Override(value) = maybe_value {
                value.emit_value(sink)?;
            }
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
    V: EmitValue,
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

impl EmitValue for bool {
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

impl EmitValue for f64 {
    emit_value_numeric!();
}

impl EmitValue for u32 {
    emit_value_numeric!();
}

impl EmitValue for i32 {
    emit_value_numeric!();
}

impl EmitValue for String {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        sink.write_str(self)
    }
}

impl EmitValue for subtitle::Alignment {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.as_an().emit_value(sink)
    }
}

impl EmitValue for subtitle::WrapStyle {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        (*self as i32).emit_value(sink)
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
