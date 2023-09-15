use crate::subtitle;

mod emit;

/// Tags that apply to the entire line, may only be used once,
/// and that only make sense to put at the beginning of the line.
#[derive(Debug, Clone, Default)]
pub struct Global {
    position: Option<PositionOrMove>,
    origin: Option<Position>,
    fade: Option<Fade>,
    wrap_style: Option<subtitle::WrapStyle>,
    animation: Option<Animation<GlobalAnimatable>>,
}

impl Global {
    pub fn emit<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        // TODO: other global properties

        emit::simple_tag(sink, "q", &self.wrap_style)?;

        Ok(())
    }
}

/// Subset of global tags that are animatable.
#[derive(Debug, Clone, Default)]
pub struct GlobalAnimatable {
    clip: Option<AnimatableClip>,
}

/// Tags that modify the text following it.
#[derive(Debug, Clone, Default)]
pub struct Local {
    italic: Option<bool>,
    font_weight: Option<FontWeight>,
    underline: Option<bool>,
    strike_out: Option<bool>,

    /// Set the size of the border around the line. Maps to `\xbord` and `\ybord`.
    border: Maybe2D,

    /// Set the drop shadow offset. Maps to `\xshad` and `\yshad`. If both are 0, no shadow will
    /// be drawn.
    shadow: Maybe2D,

    blur: Option<Blur>,

    font_name: Option<String>,
    font_size: Option<f64>,
    font_scale: Maybe2D,
    letter_spacing: Option<f64>,

    text_rotation: Maybe3D,
    text_shear: Maybe2D,

    font_encoding: Option<i32>,

    primary_colour: Option<Colour>,
    secondary_colour: Option<Colour>,
    border_colour: Option<Colour>,
    shadow_colour: Option<Colour>,

    primary_transparency: Option<Transparency>,
    secondary_transparency: Option<Transparency>,
    border_transparency: Option<Transparency>,
    shadow_transparency: Option<Transparency>,

    alignment: Option<subtitle::Alignment>,

    karaoke_effect: Option<KaraokeEffect>,

    /// Maps to `\kt` — sets the absolute time of a karaoke syllable.
    /// Must be combined with `karaoke_effect`.
    /// See https://aegisub.org/blog/vsfilter-hacks/
    karaoke_absolute_timing: Option<Centiseconds>,

    animation: Option<Animation<LocalAnimatable>>,
}

impl Local {
    pub fn emit<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        emit::simple_tag(sink, "i", &self.italic)?;
        emit::simple_tag(sink, "b", &self.font_weight)?;
        emit::simple_tag(sink, "u", &self.underline)?;
        emit::simple_tag(sink, "s", &self.strike_out)?;

        self.border.emit(sink, "", "bord")?;
        self.shadow.emit(sink, "", "shad")?;

        emit::tag(sink, self.blur)?;

        emit::simple_tag(sink, "fn", &self.font_name)?;
        emit::simple_tag(sink, "fs", &self.font_size)?;
        self.font_scale.emit(sink, "fsc", "")?;
        emit::simple_tag(sink, "fsp", &self.letter_spacing)?;

        self.text_rotation.emit(sink, "fr", "")?;
        self.text_shear.emit(sink, "fa", "")?;

        emit::simple_tag(sink, "fe", &self.font_encoding)?;

        emit::simple_tag(sink, "1c", &self.primary_colour)?;
        emit::simple_tag(sink, "2c", &self.secondary_colour)?;
        emit::simple_tag(sink, "3c", &self.border_colour)?;
        emit::simple_tag(sink, "4c", &self.shadow_colour)?;

        emit::simple_tag(sink, "1a", &self.primary_transparency)?;
        emit::simple_tag(sink, "2a", &self.secondary_transparency)?;
        emit::simple_tag(sink, "3a", &self.border_transparency)?;
        emit::simple_tag(sink, "4a", &self.shadow_transparency)?;

        emit::simple_tag(sink, "an", &self.alignment)?;

        emit::tag(sink, self.karaoke_effect)?;
        emit::simple_tag(sink, "kt", &self.karaoke_absolute_timing)?;

        // TODO: animations

        Ok(())
    }
}

/// Subset of local tags that are animatable.
#[derive(Debug, Clone, Default)]
pub struct LocalAnimatable {
    border: Maybe2D,
    shadow: Maybe2D,

    blur: Option<Blur>,

    font_size: Option<f64>,
    font_scale: Maybe2D,
    letter_spacing: Option<f64>,

    text_rotation: Maybe3D,
    text_shear: Maybe2D,

    primary_colour: Option<Colour>,
    secondary_colour: Option<Colour>,
    border_colour: Option<Colour>,
    shadow_colour: Option<Colour>,

    primary_transparency: Option<Transparency>,
    secondary_transparency: Option<Transparency>,
    border_transparency: Option<Transparency>,
    shadow_transparency: Option<Transparency>,
}

#[derive(Debug, Clone, Copy)]
pub struct Milliseconds(i32);

impl emit::EmitValue for Milliseconds {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.0.emit_value(sink)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Centiseconds(f64);

impl emit::EmitValue for Centiseconds {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.0.emit_value(sink)
    }
}

#[derive(Debug, Clone)]
pub struct Animation<A> {
    modifiers: A,
    acceleration: Option<f64>,
    interval: Option<AnimationInterval>,
}

#[derive(Debug, Clone, Copy)]
pub struct AnimationInterval {
    start: Milliseconds,
    end: Milliseconds,
}

#[derive(Debug, Clone, Copy)]
pub enum PositionOrMove {
    Position(Position),
    Move(Move),
}

#[derive(Debug, Clone, Copy)]
pub struct Position {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Maybe2D {
    x: Option<f64>,
    y: Option<f64>,
}

impl Maybe2D {
    pub fn emit<W>(&self, sink: &mut W, before: &str, after: &str) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        emit::simple_tag(
            sink,
            ThreePartTagName {
                before,
                middle: "x",
                after,
            },
            &self.x,
        )?;
        emit::simple_tag(
            sink,
            ThreePartTagName {
                before,
                middle: "y",
                after,
            },
            &self.y,
        )?;

        Ok(())
    }
}

struct ThreePartTagName<'a> {
    pub before: &'a str,
    pub middle: &'a str,
    pub after: &'a str,
}

impl emit::TagName for ThreePartTagName<'_> {
    fn write_name<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        sink.write_str(self.before)?;
        sink.write_str(self.middle)?;
        sink.write_str(self.after)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Maybe3D {
    x: Option<f64>,
    y: Option<f64>,
    z: Option<f64>,
}

impl Maybe3D {
    pub fn emit<W>(&self, sink: &mut W, before: &str, after: &str) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        emit::simple_tag(
            sink,
            ThreePartTagName {
                before,
                middle: "x",
                after,
            },
            &self.x,
        )?;
        emit::simple_tag(
            sink,
            ThreePartTagName {
                before,
                middle: "y",
                after,
            },
            &self.y,
        )?;
        emit::simple_tag(
            sink,
            ThreePartTagName {
                before,
                middle: "z",
                after,
            },
            &self.z,
        )?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Colour {
    red: u8,
    green: u8,
    blue: u8,
}

impl emit::EmitValue for Colour {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        write!(
            sink,
            "&H{:02X}{:02X}{:02X}&",
            self.blue, self.green, self.red
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Transparency(u8);

impl emit::EmitValue for Transparency {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        write!(sink, "&H{:02X}&", self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Move {
    initial_position: Position,
    final_position: Position,
    start_time: Milliseconds,
    end_time: Milliseconds,
}

#[derive(Debug, Clone, Copy)]
pub enum FontWeight {
    BoldToggle(bool),
    Numeric(u32),
}

impl emit::EmitValue for FontWeight {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match *self {
            FontWeight::BoldToggle(toggle) => toggle.emit_value(sink),
            FontWeight::Numeric(weight) => weight.emit_value(sink),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Blur {
    /// Maps to `\be`.
    /// libass parses the `\be` argument as a double, but then rounds it off after adding 0.5
    /// to match certain VSFilter quirks, and clamps it to (0..127) inclusive.
    /// So an integer value can represent all possible observable libass behaviours.
    Soften(i32),

    /// Maps to `\blur`.
    Gaussian(f64),

    /// In theory, it's possible to use both `\be` and `\blur` in a line,
    /// although it's rarely desirable to do so.
    Both(i32, f64),
}

impl emit::EmitTag for Blur {
    fn emit_tag<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match *self {
            Blur::Soften(amount) => emit::simple_tag(sink, "be", &Some(amount)),
            Blur::Gaussian(amount) => emit::simple_tag(sink, "blur", &Some(amount)),
            Blur::Both(soften_amount, gaussian_amount) => {
                emit::simple_tag(sink, "be", &Some(soften_amount))?;
                emit::simple_tag(sink, "blur", &Some(gaussian_amount))?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum KaraokeEffect {
    /// Maps to `\k`.
    FillInstant(Centiseconds),

    /// Maps to `\kf`.
    FillSweep(Centiseconds),

    /// Maps to `\ko`.
    BorderInstant(Centiseconds),
}

impl emit::EmitTag for KaraokeEffect {
    fn emit_tag<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match *self {
            KaraokeEffect::FillInstant(syl_offset) => {
                emit::simple_tag(sink, "k", &Some(syl_offset))
            }
            KaraokeEffect::FillSweep(syl_offset) => emit::simple_tag(sink, "kf", &Some(syl_offset)),
            KaraokeEffect::BorderInstant(syl_offset) => {
                emit::simple_tag(sink, "ko", &Some(syl_offset))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Fade {
    /// Maps to `\fad`.
    Simple(SimpleFade),

    /// Maps to `\fade`.
    Complex(ComplexFade),
}

#[derive(Debug, Clone, Copy)]
pub struct SimpleFade {
    fade_in_duration: Milliseconds,
    fade_out_duration: Milliseconds,
}

#[derive(Debug, Clone, Copy)]
pub struct ComplexFade {
    transparency_before: u8,
    transparency_main: u8,
    transparency_after: u8,
    fade_in_start: Milliseconds,
    fade_in_end: Milliseconds,
    fade_out_start: Milliseconds,
    fade_out_end: Milliseconds,
}

#[derive(Debug, Clone)]
pub enum Clip {
    Rectangle(ClipRectangle),
    InverseRectangle(ClipRectangle),
    Vector(ClipDrawing),
    InverseVector(ClipDrawing),
}

#[derive(Debug, Clone, Copy)]
pub enum AnimatableClip {
    Rectangle(ClipRectangle),
    InverseRectangle(ClipRectangle),
}

#[derive(Debug, Clone, Copy)]
pub struct ClipRectangle {
    x1: i32,
    x2: i32,
    y1: i32,
    y2: i32,
}

#[derive(Debug, Clone)]
pub struct ClipDrawing {
    scale: Option<f64>,
    commands: String,
}

#[derive(Debug, Clone)]
pub struct Drawing {
    scale: Option<f64>,
    baseline_offset: Option<f64>,
    commands: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local() -> Result<(), std::fmt::Error> {
        let mut string = String::new();

        let local = Local {
            primary_colour: Some(Colour {
                red: 255,
                green: 127,
                blue: 0,
            }),
            ..Default::default()
        };

        local.emit(&mut string)?;

        assert_eq!(string, "\\1c&H007FFF&");

        Ok(())
    }
}
