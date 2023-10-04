use crate::subtitle;

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
    maybe_value: &Option<V>,
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
    maybe_value: &super::Resettable<V>,
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
    maybe_value: &Option<V>,
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

impl EmitValue for f64 {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        write!(sink, "{}", *self)
    }
}

impl EmitValue for u32 {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        write!(sink, "{}", *self)
    }
}

impl EmitValue for i32 {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        write!(sink, "{}", *self)
    }
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
        simple_tag(&mut string, "blub", &no_value)?;
        assert_eq!(string, "");

        Ok(())
    }

    #[test]
    fn some_values() -> Result<(), std::fmt::Error> {
        let mut string = String::new();

        let some_value: Option<i32> = Some(123);
        simple_tag(&mut string, "blub", &some_value)?;
        complex_tag(&mut string, "blubblub", &some_value)?;

        assert_eq!(string, "\\blub123\\blubblub(123)");

        Ok(())
    }
}
