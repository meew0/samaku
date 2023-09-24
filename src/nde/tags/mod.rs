use crate::subtitle;

mod emit;
mod parse;

/// Tags that apply to the entire line, may only be used once,
/// and that only make sense to put at the beginning of the line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Global {
    pub position: Option<PositionOrMove>,
    pub origin: Option<Position>,
    pub fade: Option<Fade>,
    pub wrap_style: Option<subtitle::WrapStyle>,
    pub animation: Option<Animation<GlobalAnimatable>>,
}

impl Global {
    pub fn empty() -> Self {
        Self::default()
    }

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
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GlobalAnimatable {
    pub clip: Option<AnimatableClip>,
}

/// Tags that modify the text following it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Local {
    pub italic: Option<bool>,
    pub font_weight: Option<FontWeight>,
    pub underline: Option<bool>,
    pub strike_out: Option<bool>,

    /// Set the size of the border around the line. Maps to `\xbord` and `\ybord`.
    pub border: Maybe2D,

    /// Set the drop shadow offset. Maps to `\xshad` and `\yshad`. If both are 0, no shadow will
    /// be drawn.
    pub shadow: Maybe2D,

    pub blur: Option<Blur>,

    pub font_name: Option<String>,
    pub font_size: Option<f64>,
    pub font_scale: Maybe2D,
    pub letter_spacing: Option<f64>,

    pub text_rotation: Maybe3D,
    pub text_shear: Maybe2D,

    pub font_encoding: Option<i32>,

    pub primary_colour: Option<Colour>,
    pub secondary_colour: Option<Colour>,
    pub border_colour: Option<Colour>,
    pub shadow_colour: Option<Colour>,

    pub primary_transparency: Option<Transparency>,
    pub secondary_transparency: Option<Transparency>,
    pub border_transparency: Option<Transparency>,
    pub shadow_transparency: Option<Transparency>,

    pub alignment: Option<subtitle::Alignment>,

    pub karaoke_effect: Option<KaraokeEffect>,

    /// Maps to `\kt` — sets the absolute time of a karaoke syllable.
    /// Must be combined with `karaoke_effect`.
    /// See https://aegisub.org/blog/vsfilter-hacks/
    pub karaoke_absolute_timing: Option<Centiseconds>,

    /// Baseline offset for following drawings.
    pub drawing_baseline_offset: Option<f64>,

    pub animation: Option<Animation<LocalAnimatable>>,
}

impl Local {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Sets all tags that are present in `other` to their value in `other`. Keeps all tags that
    /// are **not** present in `other` as they currently are.
    pub fn override_from(&mut self, other: &Local) {
        option_override_from(&mut self.italic, &other.italic);
        option_override_from(&mut self.font_weight, &other.font_weight);
        option_override_from(&mut self.underline, &other.underline);
        option_override_from(&mut self.strike_out, &other.strike_out);

        self.border.override_from(&other.border);
        self.shadow.override_from(&other.shadow);

        option_override_from(&mut self.blur, &other.blur);

        option_override_from(&mut self.font_name, &other.font_name);
        option_override_from(&mut self.font_size, &other.font_size);
        self.font_scale.override_from(&other.font_scale);
        option_override_from(&mut self.letter_spacing, &other.letter_spacing);

        self.text_rotation.override_from(&other.text_rotation);
        self.text_shear.override_from(&other.text_shear);

        option_override_from(&mut self.font_encoding, &other.font_encoding);

        option_override_from(&mut self.primary_colour, &other.primary_colour);
        option_override_from(&mut self.secondary_colour, &other.secondary_colour);
        option_override_from(&mut self.border_colour, &other.border_colour);
        option_override_from(&mut self.shadow_colour, &other.shadow_colour);

        option_override_from(&mut self.primary_transparency, &other.primary_transparency);
        option_override_from(
            &mut self.secondary_transparency,
            &other.secondary_transparency,
        );
        option_override_from(&mut self.border_transparency, &other.border_transparency);
        option_override_from(&mut self.shadow_transparency, &other.shadow_transparency);

        option_override_from(&mut self.alignment, &other.alignment);

        option_override_from(&mut self.karaoke_effect, &other.karaoke_effect);

        option_override_from(
            &mut self.karaoke_absolute_timing,
            &other.karaoke_absolute_timing,
        );

        option_override_from(
            &mut self.drawing_baseline_offset,
            &other.drawing_baseline_offset,
        );

        option_override_from(&mut self.animation, &other.animation);
    }

    /// Clears all tags that are present in `other`.
    pub fn clear_from(&mut self, other: &Local) {
        option_clear_from(&mut self.italic, &other.italic);
        option_clear_from(&mut self.font_weight, &other.font_weight);
        option_clear_from(&mut self.underline, &other.underline);
        option_clear_from(&mut self.strike_out, &other.strike_out);

        self.border.clear_from(&other.border);
        self.shadow.clear_from(&other.shadow);

        option_clear_from(&mut self.blur, &other.blur);

        option_clear_from(&mut self.font_name, &other.font_name);
        option_clear_from(&mut self.font_size, &other.font_size);
        self.font_scale.clear_from(&other.font_scale);
        option_clear_from(&mut self.letter_spacing, &other.letter_spacing);

        self.text_rotation.clear_from(&other.text_rotation);
        self.text_shear.clear_from(&other.text_shear);

        option_clear_from(&mut self.font_encoding, &other.font_encoding);

        option_clear_from(&mut self.primary_colour, &other.primary_colour);
        option_clear_from(&mut self.secondary_colour, &other.secondary_colour);
        option_clear_from(&mut self.border_colour, &other.border_colour);
        option_clear_from(&mut self.shadow_colour, &other.shadow_colour);

        option_clear_from(&mut self.primary_transparency, &other.primary_transparency);
        option_clear_from(
            &mut self.secondary_transparency,
            &other.secondary_transparency,
        );
        option_clear_from(&mut self.border_transparency, &other.border_transparency);
        option_clear_from(&mut self.shadow_transparency, &other.shadow_transparency);

        option_clear_from(&mut self.alignment, &other.alignment);

        option_clear_from(&mut self.karaoke_effect, &other.karaoke_effect);

        option_clear_from(
            &mut self.karaoke_absolute_timing,
            &other.karaoke_absolute_timing,
        );

        option_clear_from(
            &mut self.drawing_baseline_offset,
            &other.drawing_baseline_offset,
        );

        option_clear_from(&mut self.animation, &other.animation);
    }

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

        emit::simple_tag(sink, "pbo", &self.drawing_baseline_offset)?;

        // TODO: animations

        Ok(())
    }
}

fn option_override_from<T>(a: &mut Option<T>, b: &Option<T>)
where
    T: Clone,
{
    if let Some(b_value) = b {
        a.replace(b_value.clone());
    }
}

fn option_clear_from<T>(a: &mut Option<T>, b: &Option<T>) {
    if b.is_some() {
        a.take();
    }
}

/// Subset of local tags that are animatable.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalAnimatable {
    pub border: Maybe2D,
    pub shadow: Maybe2D,

    pub blur: Option<Blur>,

    pub font_size: Option<f64>,
    pub font_scale: Maybe2D,
    pub letter_spacing: Option<f64>,

    pub text_rotation: Maybe3D,
    pub text_shear: Maybe2D,

    pub primary_colour: Option<Colour>,
    pub secondary_colour: Option<Colour>,
    pub border_colour: Option<Colour>,
    pub shadow_colour: Option<Colour>,

    pub primary_transparency: Option<Transparency>,
    pub secondary_transparency: Option<Transparency>,
    pub border_transparency: Option<Transparency>,
    pub shadow_transparency: Option<Transparency>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Milliseconds(i32);

impl emit::EmitValue for Milliseconds {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.0.emit_value(sink)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Centiseconds(f64);

impl emit::EmitValue for Centiseconds {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.0.emit_value(sink)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation<A> {
    pub modifiers: A,
    pub acceleration: Option<f64>,
    pub interval: Option<AnimationInterval>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationInterval {
    pub start: Milliseconds,
    pub end: Milliseconds,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PositionOrMove {
    Position(Position),
    Move(Move),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Maybe2D {
    pub x: Option<f64>,
    pub y: Option<f64>,
}

impl Maybe2D {
    pub fn override_from(&mut self, other: &Maybe2D) {
        option_override_from(&mut self.x, &other.x);
        option_override_from(&mut self.y, &other.y);
    }

    pub fn clear_from(&mut self, other: &Maybe2D) {
        option_clear_from(&mut self.x, &other.x);
        option_clear_from(&mut self.y, &other.y);
    }

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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Maybe3D {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
}

impl Maybe3D {
    pub fn override_from(&mut self, other: &Maybe3D) {
        option_override_from(&mut self.x, &other.x);
        option_override_from(&mut self.y, &other.y);
        option_override_from(&mut self.z, &other.z);
    }

    pub fn clear_from(&mut self, other: &Maybe3D) {
        option_clear_from(&mut self.x, &other.x);
        option_clear_from(&mut self.y, &other.y);
        option_clear_from(&mut self.z, &other.z);
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transparency(u8);

impl emit::EmitValue for Transparency {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        write!(sink, "&H{:02X}&", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Move {
    pub initial_position: Position,
    pub final_position: Position,
    pub start_time: Milliseconds,
    pub end_time: Milliseconds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fade {
    /// Maps to `\fad`.
    Simple(SimpleFade),

    /// Maps to `\fade`.
    Complex(ComplexFade),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimpleFade {
    pub fade_in_duration: Milliseconds,
    pub fade_out_duration: Milliseconds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComplexFade {
    pub transparency_before: u8,
    pub transparency_main: u8,
    pub transparency_after: u8,
    pub fade_in_start: Milliseconds,
    pub fade_in_end: Milliseconds,
    pub fade_out_start: Milliseconds,
    pub fade_out_end: Milliseconds,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Clip {
    Rectangle(ClipRectangle),
    InverseRectangle(ClipRectangle),
    Vector(ClipDrawing),
    InverseVector(ClipDrawing),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimatableClip {
    Rectangle(ClipRectangle),
    InverseRectangle(ClipRectangle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipRectangle {
    pub x1: i32,
    pub x2: i32,
    pub y1: i32,
    pub y2: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipDrawing {
    pub scale: Option<f64>,
    pub commands: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawing {
    pub scale: f64,
    pub commands: String,
}

impl Drawing {
    pub fn empty() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_from() {
        let mut a = Local {
            italic: Some(true),
            font_size: Some(50.0),
            font_scale: Maybe2D {
                x: Some(123.0),
                y: None,
            },
            text_rotation: Maybe3D {
                x: Some(456.0),
                y: None,
                z: Some(789.0),
            },
            ..Local::default()
        };

        let b = Local {
            underline: Some(true),
            font_size: Some(70.0),
            font_scale: Maybe2D {
                x: Some(10.0),
                y: None,
            },
            text_rotation: Maybe3D {
                x: None,
                y: Some(20.0),
                z: Some(30.0),
            },
            ..Local::default()
        };

        a.override_from(&b);

        assert_eq!(a.italic, Some(true));
        assert_eq!(a.underline, Some(true));
        assert_eq!(a.strike_out, None); // untouched
        assert_eq!(a.font_size, Some(70.0));
        assert_eq!(a.font_scale.x, Some(10.0));
        assert_eq!(a.font_scale.y, None);
        assert_eq!(a.text_rotation.x, Some(456.0));
        assert_eq!(a.text_rotation.y, Some(20.0));
        assert_eq!(a.text_rotation.z, Some(30.0));
    }

    #[test]
    fn clear_from() {
        let mut a = Local {
            italic: Some(true),
            underline: Some(false),
            ..Local::default()
        };

        let b = Local {
            underline: Some(true),
            ..Local::default()
        };

        a.clear_from(&b);

        assert_eq!(a.italic, Some(true));
        assert_eq!(a.underline, None);
    }

    #[test]
    fn local() -> Result<(), std::fmt::Error> {
        let mut string = String::new();

        let local = Local {
            primary_colour: Some(Colour {
                red: 255,
                green: 127,
                blue: 0,
            }),
            ..Local::default()
        };

        local.emit(&mut string)?;

        assert_eq!(string, "\\1c&H007FFF&");

        Ok(())
    }
}
