#![allow(
    clippy::cast_possible_truncation,
    reason = "this module needs to convert lots of types back and forth to exactly match libass' behavior"
)]

use super::{
    lerp, AnimationInterval, Colour, ComplexFade, DecimalTransparency, Fade, FontEncoding,
    FontSize, FontSizeDelta, FontWeight, Global, Local, Milliseconds, Resettable, SimpleFade,
    Transparency,
};
use crate::nde::Span;
use crate::subtitle;

#[derive(Debug, Clone, Copy)]
pub struct TimeContext {
    pub start: subtitle::StartTime,
    pub duration: subtitle::Duration,
    pub now: subtitle::StartTime,
}

impl TimeContext {
    pub fn relative(&self) -> subtitle::Duration {
        self.now - self.start
    }
}

pub struct StyleContext {
    pub original_style: subtitle::Style,
    pub new_style: subtitle::Style,
    pub style_lookup: Box<dyn Fn(&str) -> subtitle::Style>,
}

/// Accumulates style info/overrides over time.
/// Roughly corresponds to libass' `RenderContext`.
#[derive(Debug, Clone)]
struct RenderContext {
    italic: bool,
    font_weight: FontWeight,
    underline: bool,
    strike_out: bool,

    border: Float2D,
    shadow: Float2D,

    soften: i32,
    gaussian_blur: f64,

    font_name: String,
    font_size: f64,
    font_scale: Float2D,
    letter_spacing: f64,

    text_rotation: Float3D,
    text_shear: Float2D,

    font_encoding: FontEncoding,

    primary_colour: Colour,
    secondary_colour: Colour,
    border_colour: Colour,
    shadow_colour: Colour,

    primary_transparency: Transparency,
    secondary_transparency: Transparency,
    border_transparency: Transparency,
    shadow_transparency: Transparency,

    drawing_baseline_offset: f64,

    /// Since we do not support `\ko`, karaoke can be implemented by simply
    /// changing from the primary to secondary colour at some point in the
    /// event.
    use_secondary: bool,

    fade_value: Transparency,
}

impl RenderContext {
    // Roughly corresponds to `ass_reset_render_context`
    fn reset(&mut self, style: &subtitle::Style) {
        self.primary_colour = style.primary_colour;
        self.secondary_colour = style.secondary_colour;
        self.border_colour = style.border_colour;
        self.shadow_colour = style.shadow_colour;

        self.italic = style.italic;
        self.font_weight = FontWeight::BoldToggle(style.bold);
        self.underline = style.underline;
        self.strike_out = style.strike_out;

        self.font_size = style.font_size;
        self.font_name.clone_from(&style.font_name);

        self.border.x = style.border_width;
        self.border.y = style.border_width;
        self.font_scale.x = style.scale.x;
        self.font_scale.y = style.scale.y;
        self.letter_spacing = style.spacing;
        self.soften = 0;
        self.gaussian_blur = style.blur;
        self.shadow.x = style.shadow_distance;
        self.shadow.y = style.shadow_distance;
        self.text_rotation.x = 0.0;
        self.text_rotation.y = 0.0;
        self.text_rotation.z = style.angle.0;
        self.font_encoding = style.encoding;
    }
}

impl Default for RenderContext {
    fn default() -> Self {
        RenderContext {
            italic: false,
            font_weight: FontWeight::BoldToggle(false),
            underline: false,
            strike_out: false,
            border: Float2D::default(),
            shadow: Float2D::default(),
            soften: 0,
            gaussian_blur: 0.0,
            font_name: String::new(),
            font_size: 0.0,
            font_scale: Float2D::default(),
            letter_spacing: 0.0,
            text_rotation: Float3D::default(),
            text_shear: Float2D::default(),
            font_encoding: FontEncoding(0),
            primary_colour: Colour::BLACK,
            secondary_colour: Colour::BLACK,
            border_colour: Colour::BLACK,
            shadow_colour: Colour::BLACK,
            primary_transparency: Transparency(0),
            secondary_transparency: Transparency(0),
            border_transparency: Transparency(0),
            shadow_transparency: Transparency(0),
            drawing_baseline_offset: 0.0,
            use_secondary: false,
            fade_value: Transparency(0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Float2D {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Float3D {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Bakes styles and animations into the given event.
///
/// In a nutshell, this method applies the animations and style overrides present in the given event data.
/// as they would appear at the given time.
///
/// Limitations:
/// - Karaoke sweeps are not handled (`\K` / `\kf` tags; `KaraokeEffect::FillSweep`)
/// - Effects are not handled
pub fn bake(
    time: TimeContext,
    style: &StyleContext,
    global_tags: &Global,
    overrides: &Local,
    spans: Vec<Span>,
) {
    let fade = global_tags
        .fade
        .map_or(Transparency(0), |fade| bake_fade(time, fade));

    for span in spans {
        match span {
            Span::Tags(local, text) => {}
            Span::Reset => {}
            Span::ResetToStyle(style_index) => {}
            Span::Drawing(local, drawing) => {}
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "required to exactly reproduce libass' logic here"
)]
#[expect(
    clippy::cast_sign_loss,
    reason = "required to exactly reproduce libass' logic here"
)]
fn bake_fade(time: TimeContext, fade: Fade) -> Transparency {
    let complex_fade = match fade {
        Fade::Simple(simple_fade) => convert_simple_to_complex_fade(time, simple_fade),
        Fade::Complex(complex_fade) => complex_fade,
    };

    let now: i64 = time.relative().0;

    // This logic is taken from `interpolate_alpha` in libass
    if now < i64::from(complex_fade.fade_in_start.0) {
        // Before fade in
        complex_fade.transparency_before.into()
    } else if now < i64::from(complex_fade.fade_in_end.0) {
        // During fade in
        let numerator =
            f64::from(((now as u32) - complex_fade.fade_in_start.0.cast_unsigned()).cast_signed());
        let denominator = f64::from(
            (complex_fade.fade_in_end.0.cast_unsigned()
                - complex_fade.fade_in_start.0.cast_unsigned())
            .cast_signed(),
        );
        let cf = numerator / denominator;
        #[expect(
            clippy::suboptimal_flops,
            reason = "we need the 2 roundings to exactly reproduce libass"
        )]
        let a_float = f64::from(complex_fade.transparency_before.0) * (1.0 - cf)
            + cf * f64::from(complex_fade.transparency_main.0);
        Transparency(a_float as i32)
    } else if now < i64::from(complex_fade.fade_out_start.0) {
        // Between fade in and fade out
        complex_fade.transparency_main.into()
    } else if now < i64::from(complex_fade.fade_out_end.0) {
        // During fade out
        let numerator =
            f64::from(((now as u32) - complex_fade.fade_out_start.0.cast_unsigned()).cast_signed());
        let denominator = f64::from(
            (complex_fade.fade_out_end.0.cast_unsigned()
                - complex_fade.fade_out_start.0.cast_unsigned())
            .cast_signed(),
        );
        let cf = numerator / denominator;
        #[expect(
            clippy::suboptimal_flops,
            reason = "we need the 2 roundings to exactly reproduce libass"
        )]
        let a_float = f64::from(complex_fade.transparency_main.0) * (1.0 - cf)
            + cf * f64::from(complex_fade.transparency_after.0);
        Transparency(a_float as i32)
    } else {
        // After fade out
        complex_fade.transparency_after.into()
    }
}

fn convert_simple_to_complex_fade(time: TimeContext, simple_fade: SimpleFade) -> ComplexFade {
    let fade_out_end: Milliseconds = time.duration.into();
    let fade_out_start = Milliseconds(fade_out_end.0 - simple_fade.fade_out_duration.0);

    ComplexFade {
        transparency_before: DecimalTransparency(255),
        transparency_main: DecimalTransparency(0),
        transparency_after: DecimalTransparency(255),
        fade_in_start: Milliseconds(0),
        fade_in_end: simple_fade.fade_in_duration,
        fade_out_start,
        fade_out_end,
    }
}

fn apply_fade(transparency: &mut Transparency, fade: Transparency) {
    if fade.0 > 0 {
        let mult_result = mult_alpha(u32::from(transparency.rendered()), fade.0.cast_unsigned());
        *transparency = Transparency(mult_result.cast_signed());
    }
}

fn mult_alpha(first: u32, second: u32) -> u32 {
    let result_u64 = u64::from(first)
        - (u64::from(first) * u64::from(second) + 0x7f_u64) / 0xff_u64
        + u64::from(second);
    result_u64 as u32
}

fn bake_local(
    time: TimeContext,
    style: &StyleContext,
    accu: &mut RenderContext,
    local: &mut Local,
) {
    let original_accu = accu.clone();

    apply_all_resettables(style, accu, local);

    animate(time, style, accu, local);

    compact_all(style, accu, &original_accu, local);
}

fn apply_all_resettables(style: &StyleContext, accu: &mut RenderContext, local: &Local) {
    apply_resettable(
        &mut accu.primary_transparency,
        local.primary_transparency,
        style.original_style.primary_transparency,
    );
}

fn animate(time: TimeContext, style: &StyleContext, accu: &mut RenderContext, local: &Local) {
    for animation in &local.animations {
        let power = calculate_power(time, animation.acceleration, animation.interval);

        animate_single(
            &mut accu.primary_transparency,
            animation.modifiers.primary_transparency,
            style.new_style.primary_transparency,
            power,
        );
    }
}

fn compact_all(
    style: &StyleContext,
    accu: &RenderContext,
    original_accu: &RenderContext,
    local: &mut Local,
) {
    macro_rules! compact {
        ($property:ident) => {
            compact!($property, &style.new_style.$property);
        };
        ($property:ident, $style_value:expr) => {
            local.$property = compact(&accu.$property, &original_accu.$property, $style_value);
        };
    }

    macro_rules! compact_2d {
        ($property:ident, $default:expr) => {
            compact_2d!($property, $default, $default);
        };
        ($property:ident, $default_x:expr, $default_y:expr) => {
            local.$property.x = compact(&accu.$property.x, &original_accu.$property.x, $default_x);
            local.$property.y = compact(&accu.$property.y, &original_accu.$property.y, $default_y);
        };
    }

    compact!(italic);
    compact!(font_weight, &FontWeight::BoldToggle(style.new_style.bold));
    compact!(underline);
    compact!(strike_out);

    compact_2d!(border, &style.new_style.border_width);
    compact_2d!(shadow, &style.new_style.shadow_distance);

    compact!(soften, &0);
    compact!(gaussian_blur, &0.0);

    compact!(font_name);

    // font size needs to be handled on its own, not using `compact`.
    local.font_size = compact_font_size(
        accu.font_size,
        original_accu.font_size,
        style.new_style.font_size,
    );

    compact_2d!(
        font_scale,
        &style.new_style.scale.x,
        &style.new_style.scale.y
    );
    compact!(letter_spacing, &style.new_style.spacing);

    compact_2d!(text_shear, &0.0);
    compact_2d!(text_rotation, &0.0);

    // frz needs to be handled separately
    local.text_rotation.z = compact(
        &accu.text_rotation.z,
        &original_accu.text_rotation.z,
        &style.new_style.angle.0,
    );

    compact!(font_encoding, &style.new_style.encoding);

    // Apply karaoke effect (by changing the primary to the secondary colour if necessary)
    let (colour, original_colour) = if accu.use_secondary {
        (accu.primary_colour, original_accu.primary_colour)
    } else {
        (accu.secondary_colour, original_accu.secondary_colour)
    };

    local.primary_colour = compact(&colour, &original_colour, &style.new_style.primary_colour);
    compact!(border_colour);
    compact!(shadow_colour);

    compact_transparency(style, accu, original_accu, local);

    #[expect(
        clippy::float_cmp,
        reason = "exact comparisons necessary to only omit the override tag when it would be exactly the same"
    )]
    let new_drawing_baseline_offset = (accu.drawing_baseline_offset
        != original_accu.drawing_baseline_offset)
        .then_some(accu.drawing_baseline_offset);
    local.drawing_baseline_offset = new_drawing_baseline_offset;

    local.animations.clear();
}

fn compact_font_size(value: f64, previous_value: f64, current_style_value: f64) -> FontSize {
    #[expect(
        clippy::float_cmp,
        reason = "exact comparisons necessary to only omit the override tag when it would be exactly the same"
    )]
    let font_size = if value == previous_value {
        FontSize::KEEP
    } else if value == current_style_value {
        FontSize::Reset(FontSizeDelta::ZERO)
    } else {
        FontSize::Set(value)
    };
    font_size
}

// Transparency needs special handling since the fade needs to be applied in each case.
fn compact_transparency(
    style: &StyleContext,
    original_accu: &RenderContext,
    accu: &RenderContext,
    local: &mut Local,
) {
    let (transparency, original_transparency) = if accu.use_secondary {
        (
            accu.primary_transparency,
            original_accu.primary_transparency,
        )
    } else {
        (
            accu.secondary_transparency,
            original_accu.secondary_transparency,
        )
    };

    let mut primary_transparency = transparency;
    apply_fade(&mut primary_transparency, accu.fade_value);
    local.primary_transparency = compact(
        &primary_transparency,
        &original_transparency,
        &style.new_style.primary_transparency,
    );
    let mut border_transparency = accu.border_transparency;
    apply_fade(&mut border_transparency, accu.fade_value);
    local.border_transparency = compact(
        &border_transparency,
        &original_accu.border_transparency,
        &style.new_style.border_transparency,
    );
    let mut shadow_transparency = accu.shadow_transparency;
    apply_fade(&mut shadow_transparency, accu.fade_value);
    local.shadow_transparency = compact(
        &shadow_transparency,
        &original_accu.shadow_transparency,
        &style.new_style.shadow_transparency,
    );
}

/// Finds a compact `Resettable` representation of the given value in its context.
fn compact<T>(value: &T, previous_value: &T, current_style_value: &T) -> Resettable<T>
where
    T: Clone + PartialEq,
{
    if value == previous_value {
        // We just set the value to whatever we had previously,
        // so we can ignore it.
        Resettable::Keep
    } else if value == current_style_value {
        // Similar idea, but we can reset it to the current style.
        Resettable::Reset
    } else {
        // True override
        Resettable::Override(value.clone())
    }
}

fn calculate_power(
    time: TimeContext,
    acceleration: f64,
    interval_option: Option<AnimationInterval>,
) -> f64 {
    let now = time.relative().0 as i32;
    let t1 = interval_option.map_or(0_i32, |interval| interval.start.0);
    let t2 = interval_option.map_or(time.duration.0 as i32, |interval| interval.end.0);
    if now < t1 {
        0.0_f64
    } else if now >= t2 {
        1.0_f64
    } else {
        let base = f64::from((now.cast_unsigned() - t1.cast_unsigned()).cast_signed());
        base.powf(acceleration)
    }
}

/// Determine the value that a tag will have after applying the given `Resettable`
/// in its context.
fn apply_resettable<T>(original_value: &mut T, target_value: Resettable<T>, current_style_value: T)
where
    T: Clone + Eq,
{
    match target_value {
        Resettable::Keep => {
            // Do not change the previous value.
        }
        Resettable::Reset => {
            // Values are always reset to the current style, not to the original style.
            // So `{\rNewStyle\b1\b}` would reset the style to the bold value of `NewStyle`.
            *original_value = current_style_value;
        }
        Resettable::Override(override_value) => *original_value = override_value,
    }
}

/// Similar to `apply_resettable`, but animated.
fn animate_single<T>(
    original_value: &mut T,
    target_value: Resettable<T>,
    current_style_value: T,
    power: f64,
) where
    T: lerp::Lerp<Output = T> + Copy,
{
    match target_value {
        Resettable::Keep => {}
        Resettable::Reset => *original_value = current_style_value,
        Resettable::Override(override_value) => {
            *original_value = original_value.lerp(override_value, power);
        }
    }
}

fn bake_local_animations() {}

fn bake_global_animations() {
    // TODO
}

fn bake_move() {}

fn bake_karaoke() {}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches2::assert_matches;

    #[test]
    fn fade() {
        let simple_fade = SimpleFade {
            fade_in_duration: Milliseconds(500),
            fade_out_duration: Milliseconds(500),
        };

        let complex_fade = ComplexFade {
            transparency_before: DecimalTransparency(100),
            transparency_main: DecimalTransparency(150),
            transparency_after: DecimalTransparency(200),
            fade_in_start: Milliseconds(250),
            fade_in_end: Milliseconds(750),
            fade_out_start: Milliseconds(1500),
            fade_out_end: Milliseconds(2500),
        };

        let mut time_context = TimeContext {
            start: subtitle::StartTime(1000),
            duration: subtitle::Duration(3000),
            now: subtitle::StartTime(1000),
        };

        assert_eq!(
            bake_fade(time_context, Fade::Simple(simple_fade)),
            Transparency(255)
        );
        assert_eq!(
            bake_fade(time_context, Fade::Complex(complex_fade)),
            Transparency(100)
        );

        time_context.now = subtitle::StartTime(1250);
        assert_eq!(
            bake_fade(time_context, Fade::Simple(simple_fade)),
            Transparency(127)
        );
        assert_eq!(
            bake_fade(time_context, Fade::Complex(complex_fade)),
            Transparency(100)
        );

        time_context.now = subtitle::StartTime(1500);
        assert_eq!(
            bake_fade(time_context, Fade::Simple(simple_fade)),
            Transparency(0)
        );
        assert_eq!(
            bake_fade(time_context, Fade::Complex(complex_fade)),
            Transparency(125)
        );

        time_context.now = subtitle::StartTime(1750);
        assert_eq!(
            bake_fade(time_context, Fade::Simple(simple_fade)),
            Transparency(0)
        );
        assert_eq!(
            bake_fade(time_context, Fade::Complex(complex_fade)),
            Transparency(150)
        );

        time_context.now = subtitle::StartTime(2000);
        assert_eq!(
            bake_fade(time_context, Fade::Simple(simple_fade)),
            Transparency(0)
        );
        assert_eq!(
            bake_fade(time_context, Fade::Complex(complex_fade)),
            Transparency(150)
        );

        time_context.now = subtitle::StartTime(2500);
        assert_eq!(
            bake_fade(time_context, Fade::Simple(simple_fade)),
            Transparency(0)
        );
        assert_eq!(
            bake_fade(time_context, Fade::Complex(complex_fade)),
            Transparency(150)
        );

        time_context.now = subtitle::StartTime(3000);
        assert_eq!(
            bake_fade(time_context, Fade::Simple(simple_fade)),
            Transparency(0)
        );
        assert_eq!(
            bake_fade(time_context, Fade::Complex(complex_fade)),
            Transparency(175)
        );

        time_context.now = subtitle::StartTime(3500);
        assert_eq!(
            bake_fade(time_context, Fade::Simple(simple_fade)),
            Transparency(0)
        );
        assert_eq!(
            bake_fade(time_context, Fade::Complex(complex_fade)),
            Transparency(200)
        );

        time_context.now = subtitle::StartTime(3750);
        assert_eq!(
            bake_fade(time_context, Fade::Simple(simple_fade)),
            Transparency(127)
        );
        assert_eq!(
            bake_fade(time_context, Fade::Complex(complex_fade)),
            Transparency(200)
        );

        time_context.now = subtitle::StartTime(4000);
        assert_eq!(
            bake_fade(time_context, Fade::Simple(simple_fade)),
            Transparency(255)
        );
        assert_eq!(
            bake_fade(time_context, Fade::Complex(complex_fade)),
            Transparency(200)
        );
    }

    #[test]
    fn compact() {
        let mut accu = RenderContext::default();
        let style = subtitle::Style::default();
        accu.reset(&style);
        accu.strike_out = true;

        let mut new_accu = accu.clone();
        new_accu.italic = true;
        new_accu.strike_out = false;

        let style_context = StyleContext {
            original_style: style.clone(),
            new_style: style,
            style_lookup: Box::new(|_| panic!()),
        };

        let mut local = Local::empty();

        compact_all(&style_context, &new_accu, &accu, &mut local);

        assert_matches!(local.underline, Resettable::Keep);
        assert_matches!(local.strike_out, Resettable::Reset);
        assert_matches!(local.italic, Resettable::Override(true));
    }
}
