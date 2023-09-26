use std::fmt::Debug;

use crate::subtitle;

mod emit;
mod parse;

/// Like an `Option`, but also represents the possibility that an ASS tag can be specified
/// in such a way that it is not set to a value defined by the tag, but to a default value.
/// This default value comes either from the line style or is hardcoded within libass.
/// For example, in a specific override tag block, the tag `\xshad` may:
///  * not be present at all (corresponding to `Keep`, since the value from previous
///    override tags will be kept),
///  * be present without an argument — `{\xshad}` — meaning that the X shadow will be
///    reset to the value specified in the style assigned to the line (corresponding to `Reset`),
///  * or be present with an argument — `{\xshad5}` — meaning that the X shadow will be
///    set to 5 pixels (corresponding to `Override(5.0)`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Resettable<T> {
    /// Keep the value of this tag the way it was specified in the last tag of the same kind,
    /// or as its default value if it was never specified before.
    #[default]
    Keep,

    /// Reset the value of this tag to its default.
    Reset,

    /// Set the value of this tag to a specific value.
    Override(T),
}

impl<T> Resettable<T> {
    fn is_set(&self) -> bool {
        matches!(self, Self::Reset | Self::Override(_))
    }

    fn is_keep(&self) -> bool {
        matches!(self, Self::Keep)
    }
}

pub trait OverrideFrom<T> {
    fn override_from(&mut self, b: &Self);
    fn clear_from(&mut self, b: &Self);
}

impl<T> OverrideFrom<T> for Option<T>
where
    T: Clone,
{
    fn override_from(&mut self, b: &Self) {
        if let Some(b_value) = b {
            self.replace(b_value.clone());
        }
    }

    fn clear_from(&mut self, b: &Self) {
        if b.is_some() {
            self.take();
        }
    }
}

impl<T> OverrideFrom<T> for Resettable<T>
where
    T: Clone,
{
    fn override_from(&mut self, b: &Self) {
        match b {
            Self::Keep => {}
            _ => {
                let _ = std::mem::replace(self, b.clone());
            }
        }
    }

    fn clear_from(&mut self, b: &Self) {
        match b {
            Self::Keep => {}
            _ => {
                let _ = std::mem::replace(self, Self::Keep);
            }
        }
    }
}

/// Tags that apply to the entire line, may only be used once,
/// and that only make sense to put at the beginning of the line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Global {
    pub position: Option<PositionOrMove>,
    pub clip: Option<Clip>,
    pub origin: Resettable<Position>,
    pub fade: Option<Fade>,
    pub wrap_style: Resettable<subtitle::WrapStyle>,
    pub alignment: Resettable<subtitle::Alignment>,
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

        emit::simple_tag_resettable(sink, "q", &self.wrap_style)?;
        emit::simple_tag_resettable(sink, "an", &self.alignment)?;

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

    /// Maps to `\be`
    pub soften: Resettable<i32>,

    /// Maps to `\blur`
    pub gaussian_blur: Resettable<f64>,

    pub font_name: Resettable<String>,
    pub font_size: Resettable<FontSize>,
    pub font_scale: Maybe2D,
    pub letter_spacing: Resettable<f64>,

    pub text_rotation: Maybe3D,
    pub text_shear: Maybe2D,

    pub font_encoding: Resettable<i32>,

    pub primary_colour: Resettable<Colour>,
    pub secondary_colour: Resettable<Colour>,
    pub border_colour: Resettable<Colour>,
    pub shadow_colour: Resettable<Colour>,

    pub primary_transparency: Resettable<Transparency>,
    pub secondary_transparency: Resettable<Transparency>,
    pub border_transparency: Resettable<Transparency>,
    pub shadow_transparency: Resettable<Transparency>,

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
        self.italic.override_from(&other.italic);
        self.font_weight.override_from(&other.font_weight);
        self.underline.override_from(&other.underline);
        self.strike_out.override_from(&other.strike_out);

        self.border.override_from(&other.border);
        self.shadow.override_from(&other.shadow);

        self.soften.override_from(&other.soften);
        self.gaussian_blur.override_from(&other.gaussian_blur);

        self.font_name.override_from(&other.font_name);
        self.font_size.override_from(&other.font_size);
        self.font_scale.override_from(&other.font_scale);
        self.letter_spacing.override_from(&other.letter_spacing);

        self.text_rotation.override_from(&other.text_rotation);
        self.text_shear.override_from(&other.text_shear);

        self.font_encoding.override_from(&other.font_encoding);

        self.primary_colour.override_from(&other.primary_colour);
        self.secondary_colour.override_from(&other.secondary_colour);
        self.border_colour.override_from(&other.border_colour);
        self.shadow_colour.override_from(&other.shadow_colour);

        self.primary_transparency
            .override_from(&other.primary_transparency);
        self.secondary_transparency
            .override_from(&other.secondary_transparency);
        self.border_transparency
            .override_from(&other.border_transparency);
        self.shadow_transparency
            .override_from(&other.shadow_transparency);

        self.karaoke_effect.override_from(&other.karaoke_effect);

        self.karaoke_absolute_timing
            .override_from(&other.karaoke_absolute_timing);

        self.drawing_baseline_offset
            .override_from(&other.drawing_baseline_offset);

        self.animation.override_from(&other.animation);
    }

    /// Clears all tags that are present in `other`.
    pub fn clear_from(&mut self, other: &Local) {
        self.italic.clear_from(&other.italic);
        self.font_weight.clear_from(&other.font_weight);
        self.underline.clear_from(&other.underline);
        self.strike_out.clear_from(&other.strike_out);

        self.border.clear_from(&other.border);
        self.shadow.clear_from(&other.shadow);

        self.soften.clear_from(&other.soften);
        self.gaussian_blur.clear_from(&other.gaussian_blur);

        self.font_name.clear_from(&other.font_name);
        self.font_size.clear_from(&other.font_size);
        self.font_scale.clear_from(&other.font_scale);
        self.letter_spacing.clear_from(&other.letter_spacing);

        self.text_rotation.clear_from(&other.text_rotation);
        self.text_shear.clear_from(&other.text_shear);

        self.font_encoding.clear_from(&other.font_encoding);

        self.primary_colour.clear_from(&other.primary_colour);
        self.secondary_colour.clear_from(&other.secondary_colour);
        self.border_colour.clear_from(&other.border_colour);
        self.shadow_colour.clear_from(&other.shadow_colour);

        self.primary_transparency
            .clear_from(&other.primary_transparency);
        self.secondary_transparency
            .clear_from(&other.secondary_transparency);
        self.border_transparency
            .clear_from(&other.border_transparency);
        self.shadow_transparency
            .clear_from(&other.shadow_transparency);

        self.karaoke_effect.clear_from(&other.karaoke_effect);

        self.karaoke_absolute_timing
            .clear_from(&other.karaoke_absolute_timing);

        self.drawing_baseline_offset
            .clear_from(&other.drawing_baseline_offset);

        self.animation.clear_from(&other.animation);
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

        emit::simple_tag_resettable(sink, "be", &self.soften)?;
        emit::simple_tag_resettable(sink, "blur", &self.gaussian_blur)?;

        emit::simple_tag_resettable(sink, "fn", &self.font_name)?;
        emit::simple_tag_resettable(sink, "fs", &self.font_size)?;
        self.font_scale.emit(sink, "fsc", "")?;
        emit::simple_tag_resettable(sink, "fsp", &self.letter_spacing)?;

        self.text_rotation.emit(sink, "fr", "")?;
        self.text_shear.emit(sink, "fa", "")?;

        emit::simple_tag_resettable(sink, "fe", &self.font_encoding)?;

        emit::simple_tag_resettable(sink, "1c", &self.primary_colour)?;
        emit::simple_tag_resettable(sink, "2c", &self.secondary_colour)?;
        emit::simple_tag_resettable(sink, "3c", &self.border_colour)?;
        emit::simple_tag_resettable(sink, "4c", &self.shadow_colour)?;

        emit::simple_tag_resettable(sink, "1a", &self.primary_transparency)?;
        emit::simple_tag_resettable(sink, "2a", &self.secondary_transparency)?;
        emit::simple_tag_resettable(sink, "3a", &self.border_transparency)?;
        emit::simple_tag_resettable(sink, "4a", &self.shadow_transparency)?;

        emit::tag(sink, self.karaoke_effect)?;
        emit::simple_tag(sink, "kt", &self.karaoke_absolute_timing)?;

        emit::simple_tag(sink, "pbo", &self.drawing_baseline_offset)?;

        // TODO: animations

        Ok(())
    }
}

/// Subset of local tags that are animatable.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalAnimatable {
    pub border: Maybe2D,
    pub shadow: Maybe2D,

    pub soften: Resettable<i32>,
    pub gaussian_blur: Resettable<f64>,

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
    pub x: Resettable<f64>,
    pub y: Resettable<f64>,
}

impl Maybe2D {
    pub fn override_from(&mut self, other: &Maybe2D) {
        self.x.override_from(&other.x);
        self.y.override_from(&other.y);
    }

    pub fn clear_from(&mut self, other: &Maybe2D) {
        self.x.clear_from(&other.x);
        self.y.clear_from(&other.y);
    }

    pub fn emit<W>(&self, sink: &mut W, before: &str, after: &str) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        emit::simple_tag_resettable(
            sink,
            ThreePartTagName {
                before,
                middle: "x",
                after,
            },
            &self.x,
        )?;
        emit::simple_tag_resettable(
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
    pub x: Resettable<f64>,
    pub y: Resettable<f64>,
    pub z: Resettable<f64>,
}

impl Maybe3D {
    pub fn override_from(&mut self, other: &Maybe3D) {
        self.x.override_from(&other.x);
        self.y.override_from(&other.y);
        self.z.override_from(&other.z);
    }

    pub fn clear_from(&mut self, other: &Maybe3D) {
        self.x.clear_from(&other.x);
        self.y.clear_from(&other.y);
        self.z.clear_from(&other.z);
    }

    pub fn emit<W>(&self, sink: &mut W, before: &str, after: &str) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        emit::simple_tag_resettable(
            sink,
            ThreePartTagName {
                before,
                middle: "x",
                after,
            },
            &self.x,
        )?;
        emit::simple_tag_resettable(
            sink,
            ThreePartTagName {
                before,
                middle: "y",
                after,
            },
            &self.y,
        )?;
        emit::simple_tag_resettable(
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
    pub timing: Option<MoveTiming>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveTiming {
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
pub enum FontSize {
    Set(f64),
    Increase(f64),
    Decrease(f64),
}

impl emit::EmitValue for FontSize {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match *self {
            FontSize::Set(font_size) => font_size.emit_value(sink),
            FontSize::Increase(delta) => {
                sink.write_char('+')?;
                delta.emit_value(sink)
            }
            FontSize::Decrease(delta) => {
                sink.write_char('-')?;
                delta.emit_value(sink)
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
    pub scale: i32,
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
        use Resettable::*;

        let mut a = Local {
            italic: Some(true),
            font_size: Override(FontSize::Set(50.0)),
            font_scale: Maybe2D {
                x: Override(123.0),
                y: Keep,
            },
            text_rotation: Maybe3D {
                x: Override(456.0),
                y: Keep,
                z: Override(789.0),
            },
            ..Local::default()
        };

        let b = Local {
            underline: Some(true),
            font_size: Override(FontSize::Set(70.0)),
            font_scale: Maybe2D {
                x: Override(10.0),
                y: Keep,
            },
            text_rotation: Maybe3D {
                x: Keep,
                y: Reset,
                z: Override(30.0),
            },
            ..Local::default()
        };

        a.override_from(&b);

        assert_eq!(a.italic, Some(true));
        assert_eq!(a.underline, Some(true));
        assert_eq!(a.strike_out, None); // untouched
        assert_eq!(a.font_size, Override(FontSize::Set(70.0)));
        assert_eq!(a.font_scale.x, Override(10.0));
        assert_eq!(a.font_scale.y, Keep);
        assert_eq!(a.text_rotation.x, Override(456.0));
        assert_eq!(a.text_rotation.y, Reset);
        assert_eq!(a.text_rotation.z, Override(30.0));
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
            primary_colour: Resettable::Override(Colour {
                red: 255,
                green: 127,
                blue: 0,
            }),
            font_size: Resettable::Reset,
            ..Local::default()
        };

        local.emit(&mut string)?;

        assert_eq!(string, "\\fs\\1c&H007FFF&");

        Ok(())
    }
}
