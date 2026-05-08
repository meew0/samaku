#![allow(
    clippy::cast_possible_truncation,
    reason = "this module needs to convert lots of types back and forth to exactly match libass' behavior"
)]

use super::*;
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
struct RenderContext {
    italic: bool,
    font_weight: FontWeight,
    underline: bool,
    strike_out: bool,

    border_x: f64,
    border_y: f64,
    shadow_x: f64,
    shadow_y: f64,

    soften: i32,
    gaussian_blur: f64,

    font_name: String,
    font_size: FontSize,
    font_scale_x: f64,
    font_scale_y: f64,
    letter_spacing: f64,

    text_rotation_x: f64,
    text_rotation_y: f64,
    text_rotation_z: f64,
    text_shear_x: f64,
    text_shear_y: f64,

    font_encoding: i32,

    primary_colour: Colour,
    secondary_colour: Colour,
    border_colour: Colour,
    shadow_colour: Colour,

    primary_transparency: Transparency,
    secondary_transparency: Transparency,
    border_transparency: Transparency,
    shadow_transparency: Transparency,

    drawing_baseline_offset: f64,

    fade_value: Transparency,
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
    style: StyleContext,
    global_tags: &Global,
    overrides: &Local,
    text: Vec<Span>,
) {
    let fade = global_tags
        .fade
        .map_or(Transparency(0), |fade| bake_fade(time, fade));

    for span in text {
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
    apply_all_resettables(style, accu, local);

    animate(time, style, accu, local);

    let mut primary_transparency = accu.primary_transparency;
    apply_fade(&mut primary_transparency, accu.fade_value);

    local.animations.clear();
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

/// Finds a compact `Resettable` representation of the given value in its context.
fn compact<T>(value: T, previous_value: T, current_style_value: T) -> Resettable<T>
where
    T: Copy + Eq,
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
        Resettable::Override(value)
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
}
