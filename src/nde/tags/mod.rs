use std::{fmt::Debug, ops::Add};

pub use emit::emit;
pub use parse::parse;
pub use parse::raw as parse_raw;

mod emit;
mod lerp;
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
    fn is_keep(&self) -> bool {
        matches!(self, Self::Keep)
    }

    /// Analogous to `Option::take`: removes and returns the current value from the `Resettable`
    /// and replaces it with `Keep`.
    #[must_use]
    pub fn take(&mut self) -> Resettable<T> {
        std::mem::replace(self, Self::Keep)
    }

    #[inline]
    pub fn as_ref(&self) -> Resettable<&T> {
        use Resettable::*;

        match *self {
            Override(ref x) => Override(x),
            Reset => Reset,
            Keep => Keep,
        }
    }
}

impl<T> lerp::Lerp for Resettable<T>
where
    T: lerp::Lerp,
{
    type Output = Resettable<T::Output>;

    fn lerp(self, other: Self, power: f64) -> Self::Output {
        match self {
            Resettable::Keep => other.out(),
            Resettable::Reset => match other {
                Resettable::Reset | Resettable::Keep => Resettable::Reset,
                Resettable::Override(value) => Resettable::Override(value.out()),
            },
            Resettable::Override(value1) => match other {
                Resettable::Keep => Resettable::Override(value1.out()),
                Resettable::Reset => Resettable::Reset,
                Resettable::Override(value2) => Resettable::Override(value1.lerp(value2, power)),
            },
        }
    }

    fn out(self) -> Self::Output {
        match self {
            Resettable::Keep => Resettable::Keep,
            Resettable::Reset => Resettable::Reset,
            Resettable::Override(value) => Resettable::Override(value.out()),
        }
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

pub trait Animatable: emit::Value {}

/// Tags that apply to the entire line, may only be used once,
/// and that only make sense to put at the beginning of the line.
#[derive(Clone, Default, PartialEq)]
pub struct Global {
    pub position: Option<PositionOrMove>,
    pub rectangle_clip: Option<Clip<Rectangle>>,
    pub vector_clip: Option<Clip<Drawing>>,
    pub origin: Option<Position>,
    pub fade: Option<Fade>,
    pub wrap_style: Resettable<WrapStyle>,
    pub alignment: Resettable<Alignment>,
    pub animations: Vec<Animation<GlobalAnimatable>>,
}

impl Global {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn split_animatable(&mut self) -> GlobalAnimatable {
        GlobalAnimatable {
            clip: self.rectangle_clip.take(),
        }
    }

    pub fn override_from(&mut self, other: &Global) {
        self.position.override_from(&other.position);
        self.rectangle_clip.override_from(&other.rectangle_clip);
        self.vector_clip.override_from(&other.vector_clip);
        self.origin.override_from(&other.origin);
        self.fade.override_from(&other.fade);
        self.wrap_style.override_from(&other.wrap_style);
        self.alignment.override_from(&other.alignment);

        self.animations.extend(other.animations.clone());
    }

    /// Emit the tags specified in `self` as ASS override tags into the given writable sink.
    ///
    /// # Errors
    /// Returns a [`std::fmt::Error`] if writing values into the sink fails.
    pub fn emit<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        emit::tag(sink, &self.position)?;
        emit::tag(sink, &self.rectangle_clip)?;
        emit::tag(sink, &self.vector_clip)?;
        emit::complex_tag(sink, "org", self.origin.as_ref())?;
        emit::tag(sink, &self.fade)?;
        emit::simple_tag_resettable(sink, "q", self.wrap_style.as_ref())?;
        emit::simple_tag_resettable(sink, "an", self.alignment.as_ref())?;

        for animation in &self.animations {
            animation.emit(sink)?;
        }

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
                .field("rectangle_clip", &self.rectangle_clip)
                .field("vector_clip", &self.vector_clip)
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
    pub clip: Option<Clip<Rectangle>>,
}

impl GlobalAnimatable {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }
}

impl Animatable for GlobalAnimatable {}

impl emit::Value for GlobalAnimatable {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        emit::tag(sink, &self.clip)?;

        Ok(())
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
    pub font_size: FontSize,
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
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn split_animatable(&mut self) -> LocalAnimatable {
        LocalAnimatable {
            border: self.border.take(),
            shadow: self.shadow.take(),
            soften: self.soften.take(),
            gaussian_blur: self.gaussian_blur.take(),
            font_size: self.font_size.take(),
            font_scale: self.font_scale.take(),
            letter_spacing: self.letter_spacing.take(),
            text_rotation: self.text_rotation.take(),
            text_shear: self.text_shear.take(),
            primary_colour: self.primary_colour.take(),
            secondary_colour: self.secondary_colour.take(),
            border_colour: self.border_colour.take(),
            shadow_colour: self.shadow_colour.take(),
            primary_transparency: self.primary_transparency.take(),
            secondary_transparency: self.secondary_transparency.take(),
            border_transparency: self.border_transparency.take(),
            shadow_transparency: self.shadow_transparency.take(),
        }
    }

    #[must_use]
    pub fn from_animatable(other: &LocalAnimatable) -> Self {
        Self {
            border: other.border,
            shadow: other.shadow,
            soften: other.soften,
            gaussian_blur: other.gaussian_blur,
            font_size: other.font_size,
            font_scale: other.font_scale,
            letter_spacing: other.letter_spacing,
            text_rotation: other.text_rotation,
            text_shear: other.text_shear,
            primary_colour: other.primary_colour,
            secondary_colour: other.secondary_colour,
            border_colour: other.border_colour,
            shadow_colour: other.shadow_colour,
            primary_transparency: other.primary_transparency,
            secondary_transparency: other.secondary_transparency,
            border_transparency: other.border_transparency,
            shadow_transparency: other.shadow_transparency,
            ..Default::default()
        }
    }

    /// Sets all tags that are present in `other` to their value in `other`. Keeps all tags that
    /// are **not** present in `other` as they currently are. “Present” is defined as
    /// `Option::Some`, `Resettable::Reset`, or `Resettable::Override`.
    ///
    /// The `merge` argument controls the behaviour of this method with respect to incrementally
    /// specifiable tags (i.e. `font_size`, karaoke effects, and animations). With `merge: true`,
    /// it will behave as if merging two subsequent tag blocks into one — that is, the effects of
    /// `other` will always be added onto `self`, if applicable. With `merge: false`, it will
    /// behave as if modifying the `self` value using a globally specified override tag — that is,
    /// if `other` specifies a relative value, `self` will only be modified if it specifies an
    /// absolute one.
    ///
    /// Animations and karaoke effects will be concatenated if `merge: true` and overwritten
    /// otherwise. For karaoke effects and `merge: false`, only the effect type will be changed,
    /// not the timing.
    pub fn override_from(&mut self, other: &Local, merge: bool) {
        self.italic.override_from(&other.italic);
        self.font_weight.override_from(&other.font_weight);
        self.underline.override_from(&other.underline);
        self.strike_out.override_from(&other.strike_out);

        self.border.override_from(&other.border);
        self.shadow.override_from(&other.shadow);

        self.soften.override_from(&other.soften);
        self.gaussian_blur.override_from(&other.gaussian_blur);

        self.font_name.override_from(&other.font_name);
        self.font_size.override_from(&other.font_size, merge);
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

        self.karaoke.override_from(&other.karaoke, merge);

        self.drawing_baseline_offset
            .override_from(&other.drawing_baseline_offset);

        if merge {
            self.animations.extend(other.animations.clone());
        } else {
            self.animations = other.animations.clone();
        }
    }

    /// Clears all tags that are present in `other`. This includes all animations if any animation
    /// is present in `other`, and all karaoke effects if any karaoke effect is present in
    /// `other`. (Karaoke onsets in `other` are ignored.)
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

        self.karaoke.clear_from(&other.karaoke);

        self.drawing_baseline_offset
            .clear_from(&other.drawing_baseline_offset);

        if !other.animations.is_empty() {
            self.animations.clear();
        }
    }

    /// Interpolates properties linearly between `self` and `other`, according to the given `power`
    /// linear interpolation parameter.
    pub fn interpolate(&mut self, other: &Local, power: f64) {
        use lerp::Lerp;

        // Animatable tags
        self.border = self.border.lerp(other.border, power);
        self.shadow = self.shadow.lerp(other.shadow, power);
        self.soften = self.soften.lerp(other.soften, power);
        self.gaussian_blur = self.gaussian_blur.lerp(other.gaussian_blur, power);
        self.font_size = self.font_size.lerp(other.font_size, power);
        self.font_scale = self.font_scale.lerp(other.font_scale, power);
        self.letter_spacing = self.letter_spacing.lerp(other.letter_spacing, power);
        self.text_rotation = self.text_rotation.lerp(other.text_rotation, power);
        self.text_shear = self.text_shear.lerp(other.text_shear, power);
        self.primary_colour = self.primary_colour.lerp(other.primary_colour, power);
        self.secondary_colour = self.secondary_colour.lerp(other.secondary_colour, power);
        self.border_colour = self.border_colour.lerp(other.border_colour, power);
        self.shadow_colour = self.shadow_colour.lerp(other.shadow_colour, power);
        self.primary_transparency = self
            .primary_transparency
            .lerp(other.primary_transparency, power);
        self.secondary_transparency = self
            .secondary_transparency
            .lerp(other.secondary_transparency, power);
        self.border_transparency = self
            .border_transparency
            .lerp(other.border_transparency, power);
        self.shadow_transparency = self
            .shadow_transparency
            .lerp(other.shadow_transparency, power);

        // Non-animatable tags which it might still make sense to interpolate
        self.font_weight = self.font_weight.lerp(other.font_weight, power);
        self.drawing_baseline_offset = self
            .drawing_baseline_offset
            .lerp(other.drawing_baseline_offset, power);
    }

    /// Emit the tags specified in `self` as ASS override tags into the given writable sink.
    ///
    /// # Errors
    /// Returns a [`std::fmt::Error`] if writing values into the sink fails.
    pub fn emit<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        emit::simple_tag_resettable(sink, "i", self.italic.as_ref())?;
        emit::simple_tag_resettable(sink, "b", self.font_weight.as_ref())?;
        emit::simple_tag_resettable(sink, "u", self.underline.as_ref())?;
        emit::simple_tag_resettable(sink, "s", self.strike_out.as_ref())?;

        self.border.emit(sink, "", "bord")?;
        self.shadow.emit(sink, "", "shad")?;

        emit::simple_tag_resettable(sink, "be", self.soften.as_ref())?;
        emit::simple_tag_resettable(sink, "blur", self.gaussian_blur.as_ref())?;

        emit::simple_tag_resettable(sink, "fn", self.font_name.as_ref())?;
        self.font_size.emit(sink)?;
        self.font_scale.emit(sink, "fsc", "")?;
        emit::simple_tag_resettable(sink, "fsp", self.letter_spacing.as_ref())?;

        self.text_rotation.emit(sink, "fr", "")?;
        self.text_shear.emit(sink, "fa", "")?;

        emit::simple_tag_resettable(sink, "fe", self.font_encoding.as_ref())?;

        emit::simple_tag_resettable(sink, "1c", self.primary_colour.as_ref())?;
        emit::simple_tag_resettable(sink, "2c", self.secondary_colour.as_ref())?;
        emit::simple_tag_resettable(sink, "3c", self.border_colour.as_ref())?;
        emit::simple_tag_resettable(sink, "4c", self.shadow_colour.as_ref())?;

        emit::simple_tag_resettable(sink, "1a", self.primary_transparency.as_ref())?;
        emit::simple_tag_resettable(sink, "2a", self.secondary_transparency.as_ref())?;
        emit::simple_tag_resettable(sink, "3a", self.border_transparency.as_ref())?;
        emit::simple_tag_resettable(sink, "4a", self.shadow_transparency.as_ref())?;

        self.karaoke.emit(sink)?;

        emit::simple_tag(sink, "pbo", self.drawing_baseline_offset.as_ref())?;

        for animation in &self.animations {
            animation.emit(sink)?;
        }

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

    pub font_size: FontSize,
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
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }
}

impl Animatable for LocalAnimatable {}

impl emit::Value for LocalAnimatable {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        Local::from_animatable(self).emit(sink)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Milliseconds(i32);

macro_rules! emit_value_newtype {
    () => {
        fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
        where
            W: std::fmt::Write,
        {
            self.0.emit_value(sink)
        }
    };
}

impl emit::Value for Milliseconds {
    emit_value_newtype!();
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Centiseconds(f64);

impl Add for Centiseconds {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl emit::Value for Centiseconds {
    emit_value_newtype!();
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation<A: Animatable> {
    pub modifiers: A,
    pub acceleration: f64,
    pub interval: Option<AnimationInterval>,
}

impl<A: Animatable> Animation<A> {
    fn emit<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        use emit::Value;

        sink.write_str("\\t(")?;
        if let Some(interval) = self.interval {
            interval.start.emit_value(sink)?;
            sink.write_char(',')?;
            interval.end.emit_value(sink)?;
            sink.write_char(',')?;
        }
        self.acceleration.emit_value(sink)?;
        sink.write_char(',')?;
        self.modifiers.emit_value(sink)?;
        sink.write_char(')')?;

        Ok(())
    }
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

impl emit::Tag for PositionOrMove {
    fn emit_tag<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match self {
            Self::Position(position) => emit::complex_tag(sink, "pos", Some(position)),
            Self::Move(move_value) => emit::complex_tag(sink, "move", Some(move_value)),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

impl emit::Value for Position {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        write!(sink, "{},{}", self.x, self.y)
    }
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

    #[must_use]
    pub fn take(&mut self) -> Maybe2D {
        Maybe2D {
            x: self.x.take(),
            y: self.y.take(),
        }
    }

    /// Emit the tags specified in `self` as ASS override tags into the given writable sink.
    ///
    /// # Errors
    /// Returns a [`std::fmt::Error`] if writing values into the sink fails.
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
            self.x.as_ref(),
        )?;
        emit::simple_tag_resettable(
            sink,
            ThreePartTagName {
                before,
                middle: "y",
                after,
            },
            self.y.as_ref(),
        )?;

        Ok(())
    }
}

impl lerp::Lerp for Maybe2D {
    type Output = Maybe2D;

    fn lerp(self, other: Self, power: f64) -> Self::Output {
        Maybe2D {
            x: self.x.lerp(other.x, power),
            y: self.y.lerp(other.y, power),
        }
    }

    fn out(self) -> Self::Output {
        self
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

    #[must_use]
    pub fn take(&mut self) -> Maybe3D {
        Maybe3D {
            x: self.x.take(),
            y: self.y.take(),
            z: self.z.take(),
        }
    }

    /// Emit the tags specified in `self` as ASS override tags into the given writable sink.
    ///
    /// # Errors
    /// Returns a [`std::fmt::Error`] if writing values into the sink fails.
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
            self.x.as_ref(),
        )?;
        emit::simple_tag_resettable(
            sink,
            ThreePartTagName {
                before,
                middle: "y",
                after,
            },
            self.y.as_ref(),
        )?;
        emit::simple_tag_resettable(
            sink,
            ThreePartTagName {
                before,
                middle: "z",
                after,
            },
            self.z.as_ref(),
        )?;

        Ok(())
    }
}

impl lerp::Lerp for Maybe3D {
    type Output = Maybe3D;

    fn lerp(self, other: Self, power: f64) -> Self::Output {
        Maybe3D {
            x: self.x.lerp(other.x, power),
            y: self.y.lerp(other.y, power),
            z: self.z.lerp(other.z, power),
        }
    }

    fn out(self) -> Self::Output {
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Colour {
    pub const WHITE: Self = Self {
        red: 255,
        green: 255,
        blue: 255,
    };

    pub const BLACK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
    };

    fn from_bgr_packed(packed: u32) -> Self {
        #[allow(clippy::unreadable_literal)]
        Self {
            red: (packed & 0x0000FF) as u8,
            green: ((packed & 0x00FF00) >> 8) as u8,
            blue: ((packed & 0xFF0000) >> 16) as u8,
        }
    }
}

impl lerp::Lerp for Colour {
    type Output = Colour;

    fn lerp(self, other: Self, power: f64) -> Self::Output {
        // TODO: colour spaces
        Colour {
            red: self.red.lerp(other.red, power),
            green: self.green.lerp(other.green, power),
            blue: self.blue.lerp(other.blue, power),
        }
    }

    fn out(self) -> Self::Output {
        self
    }
}

impl emit::Value for Colour {
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

/// A transparency value. The least significant 8 bits determine the rendered transparency:
/// 0 represents “fully opaque” and 255 represents “fully transparent”. In this way it is exactly
/// opposite to the usual idea of an alpha channel.
///
/// Note that like in libass, this is internally represented as a 32-bit signed integer and only
/// truncated on render. This allows complex wrapping animations and the like. If you need the
/// “rendered” 8-bit value, use the [`rendered`] function.  
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transparency(pub i32);

impl Transparency {
    pub const OPAQUE: Transparency = Transparency(0);

    /// Returns the lowest 8 bits of the transparency value, corresponding to how it would be shown
    /// on render.
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    pub fn rendered(self) -> u8 {
        self.0 as u8
    }
}

impl lerp::Lerp for Transparency {
    type Output = Transparency;

    fn lerp(self, other: Self, power: f64) -> Self::Output {
        Transparency(i32::from(self.rendered()).lerp(other.0, power))
    }

    fn out(self) -> Self::Output {
        self
    }
}

impl emit::Value for Transparency {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        write!(sink, "&H{:X}&", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alignment {
    pub vertical: VerticalAlignment,
    pub horizontal: HorizontalAlignment,
}

impl Alignment {
    /// Try to convert an old, SSA-style, packed (non-numpad) alignment value into an [`Alignment`].
    /// Returns `None` if the input value is not a valid packed alignment.
    #[must_use]
    pub fn try_unpack(packed: i32) -> Option<Self> {
        let vertical_opt: Option<VerticalAlignment> = match packed & 0b1100 {
            x if x == VerticalAlignment::Sub as i32 => Some(VerticalAlignment::Sub),
            x if x == VerticalAlignment::Center as i32 => Some(VerticalAlignment::Center),
            x if x == VerticalAlignment::Top as i32 => Some(VerticalAlignment::Top),
            _ => None,
        };

        let horizontal_opt: Option<HorizontalAlignment> = match packed & 0b0011 {
            x if x == HorizontalAlignment::Left as i32 => Some(HorizontalAlignment::Left),
            x if x == HorizontalAlignment::Center as i32 => Some(HorizontalAlignment::Center),
            x if x == HorizontalAlignment::Right as i32 => Some(HorizontalAlignment::Right),
            _ => None,
        };

        match vertical_opt {
            Some(vertical) => horizontal_opt.map(|horizontal| Self {
                vertical,
                horizontal,
            }),
            None => None,
        }
    }

    /// Try to convert a numpad-style alignment into an [`Alignment`]. Returns `None` if the input
    /// value is not a valid numpad-style alignment.
    #[must_use]
    pub fn try_from_an(an: i32) -> Option<Self> {
        match an {
            1 => Some(Self {
                vertical: VerticalAlignment::Sub,
                horizontal: HorizontalAlignment::Left,
            }),
            2 => Some(Self {
                vertical: VerticalAlignment::Sub,
                horizontal: HorizontalAlignment::Center,
            }),
            3 => Some(Self {
                vertical: VerticalAlignment::Sub,
                horizontal: HorizontalAlignment::Right,
            }),
            4 => Some(Self {
                vertical: VerticalAlignment::Center,
                horizontal: HorizontalAlignment::Left,
            }),
            5 => Some(Self {
                vertical: VerticalAlignment::Center,
                horizontal: HorizontalAlignment::Center,
            }),
            6 => Some(Self {
                vertical: VerticalAlignment::Center,
                horizontal: HorizontalAlignment::Right,
            }),
            7 => Some(Self {
                vertical: VerticalAlignment::Top,
                horizontal: HorizontalAlignment::Left,
            }),
            8 => Some(Self {
                vertical: VerticalAlignment::Top,
                horizontal: HorizontalAlignment::Center,
            }),
            9 => Some(Self {
                vertical: VerticalAlignment::Top,
                horizontal: HorizontalAlignment::Right,
            }),
            _ => None,
        }
    }

    /// Convert to a number to be used in the `\an` formatting tag.
    #[must_use]
    pub fn as_an(&self) -> i32 {
        match self.vertical {
            VerticalAlignment::Sub => match self.horizontal {
                HorizontalAlignment::Left => 1,
                HorizontalAlignment::Center => 2,
                HorizontalAlignment::Right => 3,
            },
            VerticalAlignment::Center => match self.horizontal {
                HorizontalAlignment::Left => 4,
                HorizontalAlignment::Center => 5,
                HorizontalAlignment::Right => 6,
            },
            VerticalAlignment::Top => match self.horizontal {
                HorizontalAlignment::Left => 7,
                HorizontalAlignment::Center => 8,
                HorizontalAlignment::Right => 9,
            },
        }
    }

    #[must_use]
    pub fn pack(&self) -> i32 {
        self.vertical as i32 | self.horizontal as i32
    }
}

impl Default for Alignment {
    fn default() -> Self {
        Self {
            vertical: VerticalAlignment::Sub,
            horizontal: HorizontalAlignment::Center,
        }
    }
}

impl emit::Value for Alignment {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.as_an().emit_value(sink)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlignment {
    Sub = 0,
    Center = 4,
    Top = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalAlignment {
    Left = 1,
    Center = 2,
    Right = 3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Move {
    pub initial_position: Position,
    pub final_position: Position,
    pub timing: Option<MoveTiming>,
}

impl emit::Value for Move {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.initial_position.emit_value(sink)?;
        sink.write_char(',')?;
        self.final_position.emit_value(sink)?;

        if let Some(timing) = self.timing {
            sink.write_char(',')?;
            timing.start_time.emit_value(sink)?;
            sink.write_char(',')?;
            timing.end_time.emit_value(sink)?;
        }

        Ok(())
    }
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

impl FontWeight {
    #[must_use]
    pub fn weight(&self) -> u32 {
        match *self {
            FontWeight::BoldToggle(true) => 700,
            FontWeight::BoldToggle(false) => 400,
            FontWeight::Numeric(weight) => weight,
        }
    }
}

impl lerp::Lerp for FontWeight {
    type Output = FontWeight;

    fn lerp(self, other: Self, power: f64) -> Self::Output {
        FontWeight::Numeric(self.weight().lerp(other.weight(), power))
    }

    fn out(self) -> Self::Output {
        self
    }
}

impl emit::Value for FontWeight {
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
    /// Apply the given delta to the previously set font size.
    Delta(FontSizeDelta),

    /// Reset the font size to the style default, then apply the given delta.
    /// The delta may be zero, in order to only reset the font size.
    Reset(FontSizeDelta),

    /// Set the font size to a specific value. The value must be strictly positive; otherwise,
    /// it will have the same effect as `Reset` with a zero delta.
    Set(f64),
}

impl FontSize {
    pub const KEEP: FontSize = FontSize::Delta(FontSizeDelta::ZERO);

    fn override_from(&mut self, other: &Self, merge: bool) {
        use FontSize::*;

        // See the `font_size_override` test for a detailed specification of this method's
        // behaviour.
        *self = match *other {
            Delta(delta2) => match *self {
                Delta(delta1) => {
                    if merge {
                        Delta(delta1 + delta2)
                    } else {
                        Delta(delta1)
                    }
                }
                Reset(delta1) => Reset(delta1 + delta2),
                Set(val) => Set(val + delta2.0),
            },
            reset_or_set => reset_or_set,
        }
    }

    fn clear_from(&mut self, other: &Self) {
        if !other.is_empty() {
            *self = Self::KEEP;
        }
    }

    fn take(&mut self) -> Self {
        std::mem::replace(self, FontSize::KEEP)
    }

    fn is_empty(&self) -> bool {
        matches!(*self, Self::Delta(delta) if delta == FontSizeDelta::ZERO)
    }

    fn emit<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        let mut delta_value: f64 = 0.0;

        match *self {
            FontSize::Delta(delta) => {
                delta_value = delta.0;
                Ok(()) // tag will be emitted in the next step
            }
            FontSize::Reset(delta) => {
                delta_value = delta.0;
                emit::simple_tag_resettable(sink, "fs", Resettable::Reset::<&EmitFontSize>)
            }
            FontSize::Set(font_size) => {
                let emit_value = EmitFontSize::Set(font_size.max(0.0));
                emit::simple_tag(sink, "fs", Some(&emit_value))
            }
        }?;

        let emit_value = match delta_value {
            negative if negative < 0.0 => EmitFontSize::Decrease(-negative),
            positive if positive > 0.0 => EmitFontSize::Increase(positive),
            _ => return Ok(()), // do not emit any other tag for a delta of zero
        };
        emit::simple_tag(sink, "fs", Some(&emit_value))
    }
}

impl lerp::Lerp for FontSize {
    type Output = FontSize;

    fn lerp(self, other: Self, power: f64) -> Self::Output {
        match self {
            FontSize::Delta(delta1) => match other {
                FontSize::Delta(delta2) => {
                    FontSize::Delta(FontSizeDelta(delta1.0.lerp(delta2.0, power)))
                }
                _ => other,
            },
            FontSize::Reset(_) => other,
            FontSize::Set(font_size1) => match other {
                FontSize::Delta(delta) => FontSize::Set(font_size1 + delta.0 * power),
                FontSize::Reset(delta) => FontSize::Reset(delta),
                FontSize::Set(font_size2) => FontSize::Set(font_size1.lerp(font_size2, power)),
            },
        }
    }

    fn out(self) -> Self::Output {
        self
    }
}

impl Default for FontSize {
    fn default() -> Self {
        Self::KEEP
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FontSizeDelta(f64);

impl FontSizeDelta {
    pub const ZERO: FontSizeDelta = FontSizeDelta(0.0);
}

impl Add for FontSizeDelta {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

enum EmitFontSize {
    Set(f64),
    Increase(f64),
    Decrease(f64),
}

impl emit::Value for EmitFontSize {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match *self {
            Self::Set(font_size) => font_size.emit_value(sink),
            Self::Increase(delta) => {
                sink.write_char('+')?;
                delta.emit_value(sink)
            }
            Self::Decrease(delta) => {
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
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Create a new `Karaoke` instance from the given values. The karaoke invariant must be
    /// upheld: if `onset` is [`KaraokeOnset::RelativeDelay`], `effect` must not be `None`.
    ///
    /// # Errors
    /// Returns [`KaraokeError::EffectRequiredForRelativeOnset`] if the karaoke invariant is
    /// violated.
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

    #[must_use]
    pub fn effect(&self) -> Option<(KaraokeEffect, Centiseconds)> {
        self.effect
    }

    #[must_use]
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

    fn override_from(&mut self, other: &Self, merge: bool) {
        if merge {
            match other.onset {
                KaraokeOnset::NoDelay => {
                    if let Some((effect, duration)) = other.effect {
                        self.add_relative(effect, duration);
                    }
                }
                KaraokeOnset::RelativeDelay(delay) => {
                    let (effect, duration) = other.effect.expect("Karaoke invariant was violated: RelativeDelay onset must only be specified with an effect present");
                    self.add_relative(effect, delay);
                    self.add_relative(effect, duration);
                }
                KaraokeOnset::Absolute(delay) => {
                    self.set_absolute(delay);

                    if let Some((effect, duration)) = other.effect {
                        self.add_relative(effect, duration);
                    }
                }
            }
        } else {
            // Only overwrite the effect type, and only if both `self` and `other` have an effect
            // set.
            if let Some((other_effect, _)) = other.effect {
                if let Some((self_effect, _)) = &mut self.effect {
                    *self_effect = other_effect;
                }
            }
        }
    }

    fn clear_from(&mut self, other: &Self) {
        // Only clear if an effect is present. We don't care about the other's onset here.
        if other.effect.is_some() {
            self.onset = KaraokeOnset::NoDelay;
            self.effect = None;
        }
    }

    fn emit<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match self.onset {
            KaraokeOnset::NoDelay => Ok(()),
            KaraokeOnset::RelativeDelay(delay) => emit::simple_tag(sink, "k", Some(&delay)),
            KaraokeOnset::Absolute(delay) => emit::simple_tag(sink, "kt", Some(&delay)),
        }?;
        match self.effect {
            None => Ok(()),
            Some((KaraokeEffect::FillInstant, duration)) => {
                emit::simple_tag(sink, "k", Some(&duration))
            }
            Some((KaraokeEffect::FillSweep, duration)) => {
                emit::simple_tag(sink, "kf", Some(&duration))
            }
            Some((KaraokeEffect::BorderInstant, duration)) => {
                emit::simple_tag(sink, "ko", Some(&duration))
            }
        }
    }
}

pub enum KaraokeError {
    /// Creating a `Karaoke` object with relative-delay
    /// onset requires specifying an effect. See
    /// `Karaoke` docs for details
    EffectRequiredForRelativeOnset,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum KaraokeOnset {
    /// There is no delay between the end of the previous karaoke effect and this onset.
    #[default]
    NoDelay,

    /// Delay the onset of this karaoke effect by the specified amount of centiseconds
    /// relative to the previous karaoke effect.
    /// Note that it is valid to specify zero centiseconds here, mapping to `\k0`,
    /// with subtly different behaviour from `NoDelay`. (TODO: document this subtly different
    /// behaviour)
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

impl emit::Tag for Fade {
    fn emit_tag<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match self {
            Self::Simple(simple) => emit::complex_tag(sink, "fad", Some(simple)),
            Self::Complex(complex) => emit::complex_tag(sink, "fade", Some(complex)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimpleFade {
    pub fade_in_duration: Milliseconds,
    pub fade_out_duration: Milliseconds,
}

impl emit::Value for SimpleFade {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.fade_in_duration.emit_value(sink)?;
        sink.write_char(',')?;
        self.fade_out_duration.emit_value(sink)?;

        Ok(())
    }
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

impl emit::Value for ComplexFade {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.transparency_before.emit_value(sink)?;
        sink.write_char(',')?;
        self.transparency_main.emit_value(sink)?;
        sink.write_char(',')?;
        self.transparency_after.emit_value(sink)?;
        sink.write_char(',')?;
        self.fade_in_start.emit_value(sink)?;
        sink.write_char(',')?;
        self.fade_in_end.emit_value(sink)?;
        sink.write_char(',')?;
        self.fade_out_start.emit_value(sink)?;
        sink.write_char(',')?;
        self.fade_out_end.emit_value(sink)?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Clip<T: emit::Value> {
    /// Show only the content **inside** the border. Maps to `\clip`.
    Contained(T),

    /// Show only the content **outside** the border. Maps to `\iclip`.
    Inverse(T),
}

impl<T: emit::Value> Clip<T> {
    pub fn is_inverse(&self) -> bool {
        matches!(self, Clip::Inverse(_))
    }
}

impl<T: emit::Value> emit::Tag for Clip<T> {
    fn emit_tag<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match self {
            Clip::Contained(structure) => emit::complex_tag(sink, "clip", Some(structure)),
            Clip::Inverse(structure) => emit::complex_tag(sink, "iclip", Some(structure)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rectangle {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

impl emit::Value for Rectangle {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        write!(sink, "{},{},{},{}", self.x1, self.y1, self.x2, self.y2)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drawing {
    pub scale: i32,
    pub commands: String,
}

impl Drawing {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl emit::Value for Drawing {
    /// Only valid for vector clips, not for inline drawings
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        write!(sink, "{},{}", self.scale, self.commands)
    }
}

/// See <http://www.tcax.org/docs/ass-specs.htm>
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapStyle {
    SmartEven = 0,
    EndOfLine = 1,
    None = 2,
    SmartLower = 3,
}

impl From<i32> for WrapStyle {
    fn from(value: i32) -> Self {
        match value {
            x if x == Self::SmartEven as i32 => Self::SmartEven,
            x if x == Self::EndOfLine as i32 => Self::EndOfLine,
            x if x == Self::None as i32 => Self::None,
            x if x == Self::SmartLower as i32 => Self::SmartLower,
            _ => Self::SmartEven,
        }
    }
}

impl emit::Value for WrapStyle {
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

    macro_rules! assert_emits {
        ($a:expr, $b:expr) => {
            let mut _str = String::new();
            $a.emit(&mut _str)?;
            assert_eq!(_str, $b);
        };
    }

    #[test]
    fn override_from() {
        use Resettable::*;

        let mut a = Local {
            font_size: FontSize::Set(50.0),
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
            font_size: FontSize::Set(70.0),
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

        a.override_from(&b, false);

        assert_eq!(a.drawing_baseline_offset, Some(2.0));
        assert_eq!(a.strike_out, Keep); // untouched
        assert_eq!(a.font_size, FontSize::Set(70.0));
        assert_eq!(a.font_scale.x, Override(10.0));
        assert_eq!(a.font_scale.y, Keep);
        assert_eq!(a.text_rotation.x, Override(456.0));
        assert_eq!(a.text_rotation.y, Reset);
        assert_eq!(a.text_rotation.z, Override(30.0));
    }

    macro_rules! fsot {
        ($merge:expr, $a:expr, $b:expr, $res:expr) => {
            let mut _a = $a;
            _a.override_from(&$b, $merge);
            assert_eq!(_a, $res);
        };
    }

    #[test]
    fn font_size_override() {
        use FontSize::*;
        const ZERO: FontSizeDelta = FontSizeDelta(0.0);
        const ONE: FontSizeDelta = FontSizeDelta(1.0);
        const TWO: FontSizeDelta = FontSizeDelta(2.0);

        fsot!(false, Delta(ZERO), Delta(ONE), Delta(ZERO));
        fsot!(false, Delta(ZERO), Reset(ONE), Reset(ONE));
        fsot!(false, Delta(ZERO), Set(1.0), Set(1.0));

        fsot!(false, Reset(ONE), Delta(ONE), Reset(TWO));
        fsot!(false, Reset(ONE), Reset(ONE), Reset(ONE));
        fsot!(false, Reset(ONE), Set(1.0), Set(1.0));

        fsot!(false, Set(1.0), Delta(ONE), Set(2.0));
        fsot!(false, Set(1.0), Reset(ONE), Reset(ONE));
        fsot!(false, Set(1.0), Set(1.0), Set(1.0));

        fsot!(true, Delta(ZERO), Delta(ONE), Delta(ONE));
        // !
        fsot!(true, Delta(ZERO), Reset(ONE), Reset(ONE));
        fsot!(true, Delta(ZERO), Set(1.0), Set(1.0));

        fsot!(true, Reset(ONE), Delta(ONE), Reset(TWO));
        fsot!(true, Reset(ONE), Reset(ONE), Reset(ONE));
        fsot!(true, Reset(ONE), Set(1.0), Set(1.0));

        fsot!(true, Set(1.0), Delta(ONE), Set(2.0));
        fsot!(true, Set(1.0), Reset(ONE), Reset(ONE));
        fsot!(true, Set(1.0), Set(1.0), Set(1.0));
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
    fn interpolate() {
        let mut local1 = Local {
            font_weight: Resettable::Override(FontWeight::BoldToggle(false)),
            border: Maybe2D {
                x: Resettable::Override(2.0),
                y: Resettable::Reset,
            },
            soften: Resettable::Keep,
            gaussian_blur: Resettable::Override(3.0),
            font_size: FontSize::Set(20.0),
            font_scale: Maybe2D {
                x: Resettable::Reset,
                y: Resettable::Keep,
            },
            text_rotation: Maybe3D {
                x: Resettable::Override(4.0),
                y: Resettable::Reset,
                z: Resettable::Keep,
            },
            primary_colour: Resettable::Override(Colour {
                red: 10,
                green: 20,
                blue: 30,
            }),
            primary_transparency: Resettable::Override(Transparency(40)),
            drawing_baseline_offset: Some(20.0),
            ..Default::default()
        };

        let local2 = Local {
            font_weight: Resettable::Override(FontWeight::Numeric(600)),
            border: Maybe2D {
                x: Resettable::Override(10.0),
                y: Resettable::Override(11.0),
            },
            soften: Resettable::Override(12),
            gaussian_blur: Resettable::Reset,
            font_size: FontSize::Delta(FontSizeDelta(10.0)),
            font_scale: Maybe2D {
                x: Resettable::Reset,
                y: Resettable::Reset,
            },
            text_rotation: Maybe3D {
                x: Resettable::Keep,
                y: Resettable::Keep,
                z: Resettable::Keep,
            },
            primary_colour: Resettable::Override(Colour {
                red: 100,
                green: 200,
                blue: 10,
            }),
            primary_transparency: Resettable::Override(Transparency(1040)),
            drawing_baseline_offset: Some(50.0),
            ..Default::default()
        };

        local1.interpolate(&local2, 0.5);

        assert_eq!(
            local1.font_weight,
            Resettable::Override(FontWeight::Numeric(500))
        );

        assert_eq!(local1.border.x, Resettable::Override(6.0));
        assert_eq!(local1.border.y, Resettable::Override(11.0));
        assert_eq!(local1.soften, Resettable::Override(12));
        assert_eq!(local1.gaussian_blur, Resettable::Reset);
        assert_eq!(local1.font_size, FontSize::Set(25.0));
        assert_eq!(local1.font_scale.x, Resettable::Reset);
        assert_eq!(local1.font_scale.y, Resettable::Reset);
        assert_eq!(local1.text_rotation.x, Resettable::Override(4.0));
        assert_eq!(local1.text_rotation.y, Resettable::Reset);
        assert_eq!(local1.text_rotation.z, Resettable::Keep);
        assert_eq!(
            local1.primary_colour,
            Resettable::Override(Colour {
                red: 55,
                green: 110,
                blue: 20,
            })
        );
        assert_eq!(
            local1.primary_transparency,
            Resettable::Override(Transparency(540))
        );
        assert_eq!(local1.drawing_baseline_offset, Some(35.0));
    }

    #[test]
    fn global() -> Result<(), std::fmt::Error> {
        let global = Global {
            position: Some(PositionOrMove::Position(Position { x: 1.0, y: 2.0 })),
            rectangle_clip: Some(Clip::Inverse(Rectangle {
                x1: 10,
                y1: 11,
                x2: 12,
                y2: 13,
            })),
            vector_clip: Some(Clip::Contained(Drawing {
                commands: "abc".to_owned(),
                scale: 1,
            })),
            origin: Some(Position { x: 3.0, y: 4.0 }),
            fade: Some(Fade::Complex(ComplexFade {
                transparency_before: 0,
                transparency_main: 100,
                transparency_after: 200,
                fade_in_start: Milliseconds(300),
                fade_in_end: Milliseconds(400),
                fade_out_start: Milliseconds(500),
                fade_out_end: Milliseconds(600),
            })),
            wrap_style: Resettable::Override(WrapStyle::SmartEven),
            alignment: Resettable::Override(Alignment {
                vertical: VerticalAlignment::Sub,
                horizontal: HorizontalAlignment::Left,
            }),
            animations: vec![],
        };

        assert_emits!(
            global,
            "\\pos(1,2)\\iclip(10,11,12,13)\\clip(1,abc)\\org(3,4)\\fade(0,100,200,300,400,500,600)\\q0\\an1"
        );

        Ok(())
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
            font_size: FontSize::Reset(FontSizeDelta::ZERO),
            ..Local::default()
        };

        local.emit(&mut string)?;

        assert_eq!(string, "\\fs\\1c&H007FFF&");

        Ok(())
    }

    #[test]
    fn colour() {
        #[allow(clippy::unreadable_literal)]
        let colour = Colour::from_bgr_packed(0xffbb11);
        assert_eq!(colour.red, 0x11);
        assert_eq!(colour.green, 0xbb);
        assert_eq!(colour.blue, 0xff);
    }

    #[test]
    fn karaoke() -> Result<(), std::fmt::Error> {
        let mut k = Karaoke::default();
        assert_eq!(k.effect, None);
        assert_emits!(k, "");

        k.add_relative(KaraokeEffect::FillInstant, Centiseconds(10.0));
        assert_eq!(
            k.effect,
            Some((KaraokeEffect::FillInstant, Centiseconds(10.0)))
        );
        assert_eq!(k.onset, KaraokeOnset::NoDelay);
        assert_emits!(k, "\\k10");

        k.add_relative(KaraokeEffect::FillSweep, Centiseconds(20.0));
        assert_eq!(
            k.effect,
            Some((KaraokeEffect::FillSweep, Centiseconds(20.0)))
        );
        assert_eq!(k.onset, KaraokeOnset::RelativeDelay(Centiseconds(10.0)));
        assert_emits!(k, "\\k10\\kf20");

        k.add_relative(KaraokeEffect::FillSweep, Centiseconds(5.0));
        assert_eq!(
            k.effect,
            Some((KaraokeEffect::FillSweep, Centiseconds(5.0)))
        );
        assert_eq!(k.onset, KaraokeOnset::RelativeDelay(Centiseconds(30.0)));
        assert_emits!(k, "\\k30\\kf5");

        k.set_absolute(Centiseconds(50.0));
        assert_eq!(k.effect, None);
        assert_eq!(k.onset, KaraokeOnset::Absolute(Centiseconds(50.0)));
        assert_emits!(k, "\\kt50");

        k.add_relative(KaraokeEffect::BorderInstant, Centiseconds(30.0));
        assert_eq!(
            k.effect,
            Some((KaraokeEffect::BorderInstant, Centiseconds(30.0)))
        );
        assert_eq!(k.onset, KaraokeOnset::Absolute(Centiseconds(50.0)));
        assert_emits!(k, "\\kt50\\ko30");

        k.add_relative(KaraokeEffect::BorderInstant, Centiseconds(40.0));
        assert_eq!(
            k.effect,
            Some((KaraokeEffect::BorderInstant, Centiseconds(40.0)))
        );
        assert_eq!(k.onset, KaraokeOnset::Absolute(Centiseconds(80.0)));
        assert_emits!(k, "\\kt80\\ko40");

        Ok(())
    }

    #[test]
    fn karaoke_override() {
        use KaraokeEffect::*;
        use KaraokeOnset::*;

        // Merge, no delay
        let mut k1 = Karaoke {
            effect: Some((FillInstant, Centiseconds(100.0))),
            onset: NoDelay,
        };
        let k2 = Karaoke {
            effect: Some((FillSweep, Centiseconds(200.0))),
            onset: NoDelay,
        };
        k1.override_from(&k2, true);
        assert_eq!(k1.effect, Some((FillSweep, Centiseconds(200.0))));
        assert_eq!(k1.onset, RelativeDelay(Centiseconds(100.0)));

        // Merge, relative delay
        let mut k1 = Karaoke {
            effect: Some((FillInstant, Centiseconds(100.0))),
            onset: RelativeDelay(Centiseconds(50.0)),
        };
        let k2 = Karaoke {
            effect: Some((FillSweep, Centiseconds(200.0))),
            onset: RelativeDelay(Centiseconds(30.0)),
        };
        k1.override_from(&k2, true);
        assert_eq!(k1.effect, Some((FillSweep, Centiseconds(200.0))));
        assert_eq!(k1.onset, RelativeDelay(Centiseconds(180.0)));

        // Merge, absolute
        let mut k1 = Karaoke {
            effect: Some((FillInstant, Centiseconds(100.0))),
            onset: Absolute(Centiseconds(50.0)),
        };
        let k2 = Karaoke {
            effect: Some((FillSweep, Centiseconds(200.0))),
            onset: Absolute(Centiseconds(30.0)),
        };
        k1.override_from(&k2, true);
        assert_eq!(k1.effect, Some((FillSweep, Centiseconds(200.0))));
        assert_eq!(k1.onset, Absolute(Centiseconds(30.0)));

        // Merge, no effect
        let mut k1 = Karaoke {
            effect: Some((FillInstant, Centiseconds(100.0))),
            onset: NoDelay,
        };
        let k2 = Karaoke {
            effect: None,
            onset: NoDelay,
        };
        k1.override_from(&k2, true);
        assert_eq!(k1.effect, Some((FillInstant, Centiseconds(100.0))));
        assert_eq!(k1.onset, NoDelay);

        // No merge
        let mut k1 = Karaoke {
            effect: Some((FillInstant, Centiseconds(100.0))),
            onset: NoDelay,
        };
        let k2 = Karaoke {
            effect: Some((FillSweep, Centiseconds(200.0))),
            onset: RelativeDelay(Centiseconds(30.0)),
        };
        k1.override_from(&k2, false);
        assert_eq!(k1.effect, Some((FillSweep, Centiseconds(100.0))));
        assert_eq!(k1.onset, NoDelay);
    }

    #[test]
    fn animations() -> Result<(), std::fmt::Error> {
        assert_emits!(
            Global {
                animations: vec![Animation {
                    modifiers: GlobalAnimatable {
                        clip: Some(Clip::Contained(Rectangle {
                            x1: 1,
                            y1: 2,
                            x2: 3,
                            y2: 4
                        }))
                    },
                    acceleration: 1.0,
                    interval: None
                }],
                ..Default::default()
            },
            "\\t(1,\\clip(1,2,3,4))"
        );

        assert_emits!(
            Local {
                animations: vec![Animation {
                    modifiers: LocalAnimatable {
                        letter_spacing: Resettable::Override(5.0),
                        ..Default::default()
                    },
                    acceleration: 1.0,
                    interval: Some(AnimationInterval {
                        start: Milliseconds(500),
                        end: Milliseconds(1000)
                    })
                }],
                ..Default::default()
            },
            "\\t(500,1000,1,\\fsp5)"
        );

        Ok(())
    }

    #[test]
    fn font_size_emit() -> Result<(), std::fmt::Error> {
        assert_emits!(FontSize::KEEP, "");
        assert_emits!(FontSize::Delta(FontSizeDelta::ZERO), "");
        assert_emits!(FontSize::Delta(FontSizeDelta(1.0)), "\\fs+1");
        assert_emits!(FontSize::Delta(FontSizeDelta(-1.0)), "\\fs-1");
        assert_emits!(FontSize::Reset(FontSizeDelta::ZERO), "\\fs");
        assert_emits!(FontSize::Reset(FontSizeDelta(1.0)), "\\fs\\fs+1");
        assert_emits!(FontSize::Reset(FontSizeDelta(-1.0)), "\\fs\\fs-1");
        assert_emits!(FontSize::Set(1.0), "\\fs1");
        assert_emits!(FontSize::Set(0.0), "\\fs0");
        assert_emits!(FontSize::Set(-1.0), "\\fs0");

        Ok(())
    }
}
