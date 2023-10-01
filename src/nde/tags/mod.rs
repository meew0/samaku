use std::{fmt::Debug, ops::Add};

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
#[derive(Clone, Default, PartialEq)]
pub struct Global {
    pub position: Option<PositionOrMove>,
    pub clip: Option<Clip>,
    pub origin: Option<Position>,
    pub fade: Option<Fade>,
    pub wrap_style: Resettable<subtitle::WrapStyle>,
    pub alignment: Resettable<subtitle::Alignment>,
    pub animations: Vec<Animation<GlobalAnimatable>>,
}

impl Global {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn animatable(&self) -> GlobalAnimatable {
        GlobalAnimatable {
            clip: self.clip.clone().and_then(Clip::into_animatable),
        }
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

impl Debug for Global {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == Global::empty() {
            write!(f, "Global {{ empty }}")
        } else {
            f.debug_struct("Global")
                .field("position", &self.position)
                .field("clip", &self.clip)
                .field("origin", &self.origin)
                .field("fade", &self.fade)
                .field("wrap_style", &self.wrap_style)
                .field("alignment", &self.alignment)
                .field("animations", &self.animations)
                .finish()
        }
    }
}

/// Subset of global tags that are animatable.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GlobalAnimatable {
    pub clip: Option<AnimatableClip>,
}

impl GlobalAnimatable {
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Tags that modify the text following it.
#[derive(Clone, Default, PartialEq)]
pub struct Local {
    pub italic: Resettable<bool>,
    pub font_weight: Resettable<FontWeight>,
    pub underline: Resettable<bool>,
    pub strike_out: Resettable<bool>,

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

    pub karaoke: Karaoke,

    /// Baseline offset for following drawings.
    pub drawing_baseline_offset: Option<f64>,

    pub animations: Vec<Animation<LocalAnimatable>>,
}

impl Local {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn animatable(&self) -> LocalAnimatable {
        LocalAnimatable {
            border: self.border,
            shadow: self.shadow,
            soften: self.soften,
            gaussian_blur: self.gaussian_blur,
            font_size: self.font_size,
            font_scale: self.font_scale,
            letter_spacing: self.letter_spacing,
            text_rotation: self.text_rotation,
            text_shear: self.text_shear,
            primary_colour: self.primary_colour,
            secondary_colour: self.secondary_colour,
            border_colour: self.border_colour,
            shadow_colour: self.shadow_colour,
            primary_transparency: self.primary_transparency,
            secondary_transparency: self.secondary_transparency,
            border_transparency: self.border_transparency,
            shadow_transparency: self.shadow_transparency,
        }
    }

    /// Sets all tags that are present in `other` to their value in `other`. Keeps all tags that
    /// are **not** present in `other` as they currently are.
    ///
    /// “Present” is defined as `Option::Some`, `Resettable::Reset`, or `Resettable::Override`.
    ///
    /// Does not modify animations and karaoke effects.
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

        self.drawing_baseline_offset
            .override_from(&other.drawing_baseline_offset);
    }

    /// Clears all tags that are present in `other`. Does not modify animations and karaoke
    /// effects.
    ///
    /// “Present” is defined as `Option::Some`, `Resettable::Reset`, or `Resettable::Override`.
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

        self.drawing_baseline_offset
            .clear_from(&other.drawing_baseline_offset);
    }

    pub fn emit<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        emit::simple_tag_resettable(sink, "i", &self.italic)?;
        emit::simple_tag_resettable(sink, "b", &self.font_weight)?;
        emit::simple_tag_resettable(sink, "u", &self.underline)?;
        emit::simple_tag_resettable(sink, "s", &self.strike_out)?;

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

        self.karaoke.emit(sink)?;

        emit::simple_tag(sink, "pbo", &self.drawing_baseline_offset)?;

        // TODO: animations

        Ok(())
    }
}

impl Debug for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == Local::empty() {
            write!(f, "Local {{ empty }}")
        } else {
            f.debug_struct("Local")
                .field("italic", &self.italic)
                .field("font_weight", &self.font_weight)
                .field("underline", &self.underline)
                .field("strike_out", &self.strike_out)
                .field("border", &self.border)
                .field("shadow", &self.shadow)
                .field("soften", &self.soften)
                .field("gaussian_blur", &self.gaussian_blur)
                .field("font_name", &self.font_name)
                .field("font_size", &self.font_size)
                .field("font_scale", &self.font_scale)
                .field("letter_spacing", &self.letter_spacing)
                .field("text_rotation", &self.text_rotation)
                .field("text_shear", &self.text_shear)
                .field("font_encoding", &self.font_encoding)
                .field("primary_colour", &self.primary_colour)
                .field("secondary_colour", &self.secondary_colour)
                .field("border_colour", &self.border_colour)
                .field("shadow_colour", &self.shadow_colour)
                .field("primary_transparency", &self.primary_transparency)
                .field("secondary_transparency", &self.secondary_transparency)
                .field("border_transparency", &self.border_transparency)
                .field("shadow_transparency", &self.shadow_transparency)
                .field("karaoke", &self.karaoke)
                .field("drawing_baseline_offset", &self.drawing_baseline_offset)
                .field("animations", &self.animations)
                .finish()
        }
    }
}

/// Subset of local tags that are animatable.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalAnimatable {
    pub border: Maybe2D,
    pub shadow: Maybe2D,

    pub soften: Resettable<i32>,
    pub gaussian_blur: Resettable<f64>,

    pub font_size: Resettable<FontSize>,
    pub font_scale: Maybe2D,
    pub letter_spacing: Resettable<f64>,

    pub text_rotation: Maybe3D,
    pub text_shear: Maybe2D,

    pub primary_colour: Resettable<Colour>,
    pub secondary_colour: Resettable<Colour>,
    pub border_colour: Resettable<Colour>,
    pub shadow_colour: Resettable<Colour>,

    pub primary_transparency: Resettable<Transparency>,
    pub secondary_transparency: Resettable<Transparency>,
    pub border_transparency: Resettable<Transparency>,
    pub shadow_transparency: Resettable<Transparency>,
}

impl LocalAnimatable {
    pub fn empty() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Milliseconds(i32);

impl emit::EmitValue for Milliseconds {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.0.emit_value(sink)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Centiseconds(f64);

impl Add for Centiseconds {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

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
    pub acceleration: f64,
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

impl Colour {
    const BLACK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
    };

    fn from_bgr_packed(packed: u32) -> Self {
        Self {
            red: (packed & 0x0000FF) as u8,
            green: ((packed & 0x00FF00) >> 8) as u8,
            blue: ((packed & 0xFF0000) >> 16) as u8,
        }
    }
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

/// Represents the effect and timing of a karaoke syllable.
/// Note that it is invalid to have a karaoke syllable
/// with no set effect (`effect: None`), but with
/// a `KaraokeOnset::RelativeDelay` onset.
/// In order to prevent this, this struct does not expose
/// public fields, but instead only getter/setter methods
/// that uphold this invariant.
/// Negative durations are supported in principle, but
/// there is no guarantee that they behave exactly as
/// they do in libass.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Karaoke {
    /// The way the syllable will be displayed, and the duration
    /// the effect will take place over.
    /// If `None`, the effect from the previous karaoke block
    /// will be used, if one exists. Otherwise there will be
    /// no effect.
    /// There is no way to unset the karaoke effect in a line
    /// after it has been set once, not even with `\r`.
    effect: Option<(KaraokeEffect, Centiseconds)>,

    /// The time point at which the effect will start to be
    /// displayed.
    onset: KaraokeOnset,
}

impl Karaoke {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn try_new(
        effect: Option<(KaraokeEffect, Centiseconds)>,
        onset: KaraokeOnset,
    ) -> Result<Self, KaraokeError> {
        if effect.is_none() && matches!(onset, KaraokeOnset::RelativeDelay(_)) {
            Err(KaraokeError::EffectRequiredForRelativeOnset)
        } else {
            Ok(Self { effect, onset })
        }
    }

    pub fn effect(&self) -> Option<(KaraokeEffect, Centiseconds)> {
        self.effect
    }

    pub fn onset(&self) -> KaraokeOnset {
        self.onset
    }

    pub fn add_relative(&mut self, effect: KaraokeEffect, duration: Centiseconds) {
        use KaraokeOnset::*;

        let old_effect = self.effect.replace((effect, duration));
        let old_duration = old_effect.map(|(_, duration)| duration);

        // Add previous duration to onset
        self.onset = match self.onset {
            NoDelay => match old_duration {
                Some(val) => RelativeDelay(val),
                None => NoDelay,
            },
            RelativeDelay(previous) => {
                RelativeDelay(previous + old_duration.unwrap_or(Centiseconds(0.0)))
            }
            Absolute(previous) => Absolute(previous + old_duration.unwrap_or(Centiseconds(0.0))),
        };
    }

    pub fn set_absolute(&mut self, absolute_delay: Centiseconds) {
        self.effect = None;
        self.onset = KaraokeOnset::Absolute(absolute_delay);
    }

    fn emit<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match self.onset {
            KaraokeOnset::NoDelay => Ok(()),
            KaraokeOnset::RelativeDelay(delay) => emit::simple_tag(sink, "k", &Some(delay)),
            KaraokeOnset::Absolute(delay) => emit::simple_tag(sink, "kt", &Some(delay)),
        }?;
        match self.effect {
            None => Ok(()),
            Some((KaraokeEffect::FillInstant, duration)) => {
                emit::simple_tag(sink, "k", &Some(duration))
            }
            Some((KaraokeEffect::FillSweep, duration)) => {
                emit::simple_tag(sink, "kf", &Some(duration))
            }
            Some((KaraokeEffect::BorderInstant, duration)) => {
                emit::simple_tag(sink, "ko", &Some(duration))
            }
        }
    }
}

enum KaraokeError {
    /// Creating a `Karaoke` object with relative-delay
    /// onset requires specifying an effect. See
    /// `Karaoke` docs for details
    EffectRequiredForRelativeOnset,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum KaraokeOnset {
    #[default]
    NoDelay,

    RelativeDelay(Centiseconds),

    /// Maps to `\kt` — sets the absolute time of a karaoke syllable.
    /// Must be combined with `karaoke_effect`.
    /// See https://aegisub.org/blog/vsfilter-hacks/
    Absolute(Centiseconds),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KaraokeEffect {
    /// Maps to `\k`.
    FillInstant,

    /// Maps to `\kf`.
    FillSweep,

    /// Maps to `\ko`.
    BorderInstant,
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

/// Before `fade_in_start`, the line will have transparency
/// `transparency_before`; between `fade_in_end` and `fade_out_start`,
/// it will have transparency `transparency_main`; and after
/// `fade_out_end`, it will have transparency `transparency_after`.
/// Between those times, it will transition linearly between
/// the respective transparency values.
///
/// Note that the transparency values have type `i32`
/// instead of the usual `u8`. They will be truncated to size `u8`,
/// but only *after* interpolation, which means that specifying
/// far larger values than 255 (or far smaller ones than 0)
/// will produce a fun wrapping effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComplexFade {
    pub transparency_before: i32,
    pub transparency_main: i32,
    pub transparency_after: i32,
    pub fade_in_start: Milliseconds,
    pub fade_in_end: Milliseconds,
    pub fade_out_start: Milliseconds,
    pub fade_out_end: Milliseconds,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Clip {
    Rectangle(ClipRectangle),
    InverseRectangle(ClipRectangle),
    Vector(Drawing),
    InverseVector(Drawing),
}

impl Clip {
    pub fn into_animatable(self) -> Option<AnimatableClip> {
        match self {
            Clip::Rectangle(rect) => Some(AnimatableClip::Rectangle(rect)),
            Clip::InverseRectangle(rect) => Some(AnimatableClip::InverseRectangle(rect)),
            _ => None,
        }
    }
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drawing {
    pub scale: i32,
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
            drawing_baseline_offset: Some(2.0),
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

        assert_eq!(a.drawing_baseline_offset, Some(2.0));
        assert_eq!(a.strike_out, Keep); // untouched
        assert_eq!(a.font_size, Override(FontSize::Set(70.0)));
        assert_eq!(a.font_scale.x, Override(10.0));
        assert_eq!(a.font_scale.y, Keep);
        assert_eq!(a.text_rotation.x, Override(456.0));
        assert_eq!(a.text_rotation.y, Reset);
        assert_eq!(a.text_rotation.z, Override(30.0));
    }

    #[test]
    fn clear_from() {
        use Resettable::*;

        let mut a = Local {
            italic: Override(true),
            underline: Override(false),
            ..Local::default()
        };

        let b = Local {
            underline: Override(true),
            ..Local::default()
        };

        a.clear_from(&b);

        assert_eq!(a.italic, Override(true));
        assert_eq!(a.underline, Keep);
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

    #[test]
    fn colour() {
        let colour = Colour::from_bgr_packed(0xffbb11);
        assert_eq!(colour.red, 0x11);
        assert_eq!(colour.green, 0xbb);
        assert_eq!(colour.blue, 0xff);
    }

    #[test]
    fn karaoke() -> Result<(), std::fmt::Error> {
        let mut k = Karaoke::default();
        assert_eq!(k.effect, None);
        let mut str = String::new();
        k.emit(&mut str)?;
        assert_eq!(str, "");

        k.add_relative(KaraokeEffect::FillInstant, Centiseconds(10.0));
        assert_eq!(
            k.effect,
            Some((KaraokeEffect::FillInstant, Centiseconds(10.0)))
        );
        assert_eq!(k.onset, KaraokeOnset::NoDelay);
        let mut str = String::new();
        k.emit(&mut str)?;
        assert_eq!(str, "\\k10");

        k.add_relative(KaraokeEffect::FillSweep, Centiseconds(20.0));
        assert_eq!(
            k.effect,
            Some((KaraokeEffect::FillSweep, Centiseconds(20.0)))
        );
        assert_eq!(k.onset, KaraokeOnset::RelativeDelay(Centiseconds(10.0)));
        let mut str = String::new();
        k.emit(&mut str)?;
        assert_eq!(str, "\\k10\\kf20");

        k.add_relative(KaraokeEffect::FillSweep, Centiseconds(5.0));
        assert_eq!(
            k.effect,
            Some((KaraokeEffect::FillSweep, Centiseconds(5.0)))
        );
        assert_eq!(k.onset, KaraokeOnset::RelativeDelay(Centiseconds(30.0)));
        let mut str = String::new();
        k.emit(&mut str)?;
        assert_eq!(str, "\\k30\\kf5");

        k.set_absolute(Centiseconds(50.0));
        assert_eq!(k.effect, None);
        assert_eq!(k.onset, KaraokeOnset::Absolute(Centiseconds(50.0)));
        let mut str = String::new();
        k.emit(&mut str)?;
        assert_eq!(str, "\\kt50");

        k.add_relative(KaraokeEffect::BorderInstant, Centiseconds(30.0));
        assert_eq!(
            k.effect,
            Some((KaraokeEffect::BorderInstant, Centiseconds(30.0)))
        );
        assert_eq!(k.onset, KaraokeOnset::Absolute(Centiseconds(50.0)));
        let mut str = String::new();
        k.emit(&mut str)?;
        assert_eq!(str, "\\kt50\\ko30");

        k.add_relative(KaraokeEffect::BorderInstant, Centiseconds(40.0));
        assert_eq!(
            k.effect,
            Some((KaraokeEffect::BorderInstant, Centiseconds(40.0)))
        );
        assert_eq!(k.onset, KaraokeOnset::Absolute(Centiseconds(80.0)));
        let mut str = String::new();
        k.emit(&mut str)?;
        assert_eq!(str, "\\kt80\\ko40");

        Ok(())
    }
}
