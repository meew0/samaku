#![allow(
    clippy::cast_possible_truncation,
    reason = "this module needs to convert lots of types back and forth to exactly match libass' behavior"
)]

use super::{
    lerp::Lerp, Animation, AnimationInterval, Clip, Colour, ComplexFade, DecimalTransparency,
    Fade, FontEncoding, FontSize, FontSizeDelta, FontWeight, Global, KaraokeEffect, KaraokeOnset,
    Local, LocalAnimatable, Milliseconds, Position, PositionOrMove, Rectangle, Resettable,
    SimpleFade, Transparency,
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
    #[must_use]
    pub fn relative(&self) -> subtitle::Duration {
        self.now - self.start
    }
}

#[derive(Debug, Clone, Copy)]
struct StyleContext<'a> {
    original_style: &'a subtitle::Style,
    new_style: &'a subtitle::Style,
}

/// Bakes styles and animations into the given event.
///
/// In a nutshell, this method applies the animations and style overrides present in the given event data.
/// as they would appear at the given time.
///
/// Limitations:
/// - Karaoke sweeps are not handled (`\K` / `\kf` tags; `KaraokeEffect::FillSweep`)
/// - The `\ko` / `KaraokeEffect:BorderInstant` karaoke effect is also not handled
/// - Some edge cases when starting a new run within a karaoke syl are not handled (for instance, automatic line breaks)
/// - Changing `BorderStyle` within a line is not supported (as this cannot even be specified with override tags)
/// - Effects (as in, marquee etc.) are not handled
///
/// The resulting spans are not yet simplified.
pub fn bake<'a, F: Fn(&str) -> Option<&'a subtitle::Style>>(
    time: TimeContext,
    event_style: &'a subtitle::Style,
    style_lookup: &'a F,
    global_tags: &mut Global,
    spans: &mut Vec<Span>,
    playback_resolution: subtitle::Resolution,
    global_overrides_option: Option<&Local>,
) {
    let mut style_context = StyleContext {
        original_style: event_style,
        new_style: event_style,
    };

    let mut accu = RenderContext::default();
    accu.reset(event_style);

    accu.fade_value = global_tags
        .fade
        .map_or(Transparency(0), |fade| bake_fade(time, fade));

    // https://github.com/libass/libass/blob/c425f6d7ec9ca7e5dfa3f8bbed29a6ddbf39a596/libass/ass_render.c#L2511
    // libass treats lines with nonzero fade slightly differently from ones without,
    // even if the transparency value would be the same.
    // So we need to assign a "force wrapped" fade to the line to achieve the same effect.
    // The difference in the final image is imperceptibly small but it exists.
    global_tags.fade = accu.fade_value.wrapped().then_some(FORCE_WRAP_FADE);

    // Similar story for this one:
    // https://github.com/libass/libass/blob/c425f6d7ec9ca7e5dfa3f8bbed29a6ddbf39a596/libass/ass_render.c#L2502
    // However we do not take this into account since there is no difference whatsoever in the composited images
    // (since they are fully transparent anyway)

    bake_global_animations(time, global_tags, playback_resolution);
    bake_move(time, global_tags);

    let karaoke_opt = has_karaoke(spans).then(|| {
        let (split_spans, respan_states) = respan(
            time,
            style_context,
            style_lookup,
            spans,
            global_overrides_option,
        );
        *spans = split_spans;

        bake_karaoke(spans, &respan_states)
    });

    for (i, span) in spans.iter_mut().enumerate() {
        if let Some(karaoke) = karaoke_opt.as_ref() {
            accu.apply_karaoke(time, karaoke[i]);
        }

        match *span {
            Span::Tags(ref mut local, _) | Span::Drawing(ref mut local, _) => {
                bake_local(
                    time,
                    style_context,
                    &mut accu,
                    local,
                    global_overrides_option,
                );
            }
            Span::Reset => {
                style_context.new_style = style_context.original_style;
                let (local, _) =
                    bake_reset(time, style_context, &mut accu, global_overrides_option);
                *span = Span::Tags(local, String::new());
            }
            Span::ResetToStyle(ref style_name) => {
                // If the new style cannot be found, libass resets it to the original style
                style_context.new_style =
                    style_lookup(style_name).unwrap_or(style_context.original_style);
                let (local, _) =
                    bake_reset(time, style_context, &mut accu, global_overrides_option);
                *span = Span::Tags(local, String::new());
            }
        }
    }
}

fn bake_local(
    time: TimeContext,
    style: StyleContext,
    accu: &mut RenderContext,
    local: &mut Local,
    global_overrides_option: Option<&Local>,
) -> RespanState {
    // First, we make a copy of the original render context, so we can compare the
    // changes that were made by the local tags.
    let original_accu = accu.clone();

    // Then, we apply the static resettable-style override tags to the render context,
    // updating all property values that are supposed to be changed.
    // This does not yet handle animations.
    accu.apply_all_resettables(style, local);

    // Now, we apply all animations in order.
    accu.animate(time, style, &local.animations);

    // Apply global overrides
    if let Some(global_overrides) = global_overrides_option {
        accu.apply_all_resettables(style, global_overrides);
        accu.animate(time, style, &global_overrides.animations);
    }

    // Finally, we take the difference between the changed render context and the
    // original one, and convert this difference into new override tags.
    accu.compact_all(local, &original_accu, style);

    accu.starts_new_run(&original_accu)
}

fn bake_reset(
    time: TimeContext,
    style: StyleContext,
    accu: &mut RenderContext,
    global_overrides_option: Option<&Local>,
) -> (Local, RespanState) {
    // This method is similar to `bake_local`, except we reset the render context
    // to `new_style`.
    let original_accu = accu.clone();
    accu.reset(style.new_style);

    // Apply global overrides
    if let Some(global_overrides) = global_overrides_option {
        accu.apply_all_resettables(style, global_overrides);
        accu.animate(time, style, &global_overrides.animations);
    }

    let mut local = Local::empty();
    accu.compact_all(&mut local, &original_accu, style);
    (local, accu.starts_new_run(&original_accu))
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

    /// Since we do not support `\kf` and `\ko`, karaoke can be implemented
    /// by simply changing from the primary to secondary colour at some point
    /// in the event.
    use_secondary: bool,

    fade_value: Transparency,

    text_colour: Colour,
    text_transparency_after_fade: Transparency,
    border_transparency_after_fade: Transparency,
    shadow_transparency_after_fade: Transparency,
}

impl RenderContext {
    // Roughly corresponds to `ass_reset_render_context`
    fn reset(&mut self, style: &subtitle::Style) {
        self.primary_transparency = style.primary_transparency;
        self.secondary_transparency = style.secondary_transparency;
        self.border_transparency = style.border_transparency;
        self.shadow_transparency = style.shadow_transparency;
        self.text_transparency_after_fade = style.primary_transparency;
        self.border_transparency_after_fade = style.border_transparency;
        self.shadow_transparency_after_fade = style.shadow_transparency;

        self.primary_colour = style.primary_colour;
        self.secondary_colour = style.secondary_colour;
        self.border_colour = style.border_colour;
        self.shadow_colour = style.shadow_colour;
        self.text_colour = style.primary_colour;

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

    fn apply_all_resettables(&mut self, style: StyleContext, local: &Local) {
        macro_rules! apply_single {
            ($property:ident) => {
                apply_single!($property, &style.new_style.$property);
            };
            ($property:ident, $style_value:expr) => {
                apply_resettable(&mut self.$property, &local.$property, $style_value);
            };
        }

        macro_rules! apply_2d {
            ($property:ident, $default:expr) => {
                apply_2d!($property, $default, $default);
            };
            ($property:ident, $default_x:expr, $default_y:expr) => {
                apply_resettable(&mut self.$property.x, &local.$property.x, $default_x);
                apply_resettable(&mut self.$property.y, &local.$property.y, $default_y);
            };
        }

        apply_single!(italic);
        apply_single!(font_weight, &FontWeight::BoldToggle(style.new_style.bold));
        apply_single!(underline);
        apply_single!(strike_out);

        apply_2d!(border, &style.new_style.border_width);
        apply_2d!(shadow, &style.new_style.shadow_distance);

        apply_single!(soften, &0);
        apply_single!(gaussian_blur, &0.0);

        apply_single!(font_name);
        animate_font_size(
            &mut self.font_size,
            local.font_size,
            style.new_style.font_size,
            1.0,
        );
        apply_2d!(
            font_scale,
            &style.new_style.scale.x,
            &style.new_style.scale.y
        );
        apply_single!(letter_spacing, &style.new_style.spacing);

        apply_2d!(text_shear, &0.0);
        apply_2d!(text_rotation, &0.0);
        apply_resettable(
            &mut self.text_rotation.z,
            &local.text_rotation.z,
            &style.new_style.angle.0,
        );

        apply_single!(font_encoding, &style.new_style.encoding);

        apply_single!(primary_colour);
        apply_single!(secondary_colour);
        apply_single!(border_colour);
        apply_single!(shadow_colour);

        apply_single!(primary_transparency);
        apply_single!(secondary_transparency);
        apply_single!(border_transparency);
        apply_single!(shadow_transparency);

        if let Some(drawing_baseline_offset) = local.drawing_baseline_offset {
            self.drawing_baseline_offset = drawing_baseline_offset;
        }
    }

    fn animate(
        &mut self,
        time: TimeContext,
        style: StyleContext,
        animations: &[Animation<LocalAnimatable>],
    ) {
        for animation in animations {
            let power = calculate_power(time, animation.acceleration, animation.interval);

            macro_rules! animate_single {
                ($property:ident) => {
                    animate_single!($property, style.new_style.$property);
                };
                ($property:ident, $style_value:expr) => {
                    animate_single(
                        &mut self.$property,
                        animation.modifiers.$property,
                        $style_value,
                        power,
                    );
                };
            }

            macro_rules! animate_2d {
                ($property:ident, $default:expr) => {
                    animate_2d!($property, $default, $default);
                };
                ($property:ident, $default_x:expr, $default_y:expr) => {
                    animate_single(
                        &mut self.$property.x,
                        animation.modifiers.$property.x,
                        $default_x,
                        power,
                    );
                    animate_single(
                        &mut self.$property.y,
                        animation.modifiers.$property.y,
                        $default_y,
                        power,
                    );
                };
            }

            animate_2d!(border, style.new_style.border_width);
            animate_2d!(shadow, style.new_style.shadow_distance);

            animate_soften(&mut self.soften, animation.modifiers.soften, 0, power);
            animate_single!(gaussian_blur, 0.0);

            animate_font_size(
                &mut self.font_size,
                animation.modifiers.font_size,
                style.new_style.font_size,
                power,
            );
            animate_2d!(font_scale, style.new_style.scale.x, style.new_style.scale.y);
            animate_single!(letter_spacing, style.new_style.spacing);

            animate_2d!(text_shear, 0.0);
            animate_2d!(text_rotation, 0.0);
            animate_single(
                &mut self.text_rotation.z,
                animation.modifiers.text_rotation.z,
                style.new_style.angle.0,
                power,
            );

            animate_single!(primary_colour);
            animate_single!(secondary_colour);
            animate_single!(border_colour);
            animate_single!(shadow_colour);

            animate_single!(primary_transparency);
            animate_single!(secondary_transparency);
            animate_single!(border_transparency);
            animate_single!(shadow_transparency);
        }
    }

    /// Turns the differences between this and the `original_accu` into override tags
    /// that are placed into `local`.
    fn compact_all(
        &mut self,
        local: &mut Local,
        original_accu: &RenderContext,
        style: StyleContext,
    ) {
        macro_rules! compact {
            ($property:ident) => {
                compact!($property, &style.original_style.$property);
            };
            ($property:ident, $style_value:expr) => {
                local.$property = compact(&self.$property, &original_accu.$property, $style_value);
            };
        }

        macro_rules! compact_2d {
            ($property:ident, $default:expr) => {
                compact_2d!($property, $default, $default);
            };
            ($property:ident, $default_x:expr, $default_y:expr) => {
                local.$property.x =
                    compact(&self.$property.x, &original_accu.$property.x, $default_x);
                local.$property.y =
                    compact(&self.$property.y, &original_accu.$property.y, $default_y);
            };
        }

        compact!(italic);
        compact!(
            font_weight,
            &FontWeight::BoldToggle(style.original_style.bold)
        );
        compact!(underline);
        compact!(strike_out);

        compact_2d!(border, &style.original_style.border_width);
        compact_2d!(shadow, &style.original_style.shadow_distance);

        compact!(soften, &0);
        // libass always resets this to 0 instead of the blur specified in the style.
        compact!(gaussian_blur, &0.0);

        compact!(font_name);

        // font size needs to be handled on its own, not using `compact`.
        local.font_size = compact_font_size(
            self.font_size,
            original_accu.font_size,
            style.original_style.font_size,
        );

        compact_2d!(
            font_scale,
            &style.original_style.scale.x,
            &style.original_style.scale.y
        );
        compact!(letter_spacing, &style.original_style.spacing);

        compact_2d!(text_shear, &0.0);
        compact_2d!(text_rotation, &0.0);

        // frz needs to be handled separately
        local.text_rotation.z = compact(
            &self.text_rotation.z,
            &original_accu.text_rotation.z,
            &style.original_style.angle.0,
        );

        compact!(font_encoding, &style.original_style.encoding);

        // Apply karaoke effect (by changing the primary to the secondary colour if necessary)
        self.text_colour = if self.use_secondary {
            self.secondary_colour
        } else {
            self.primary_colour
        };

        local.primary_colour = compact(
            &self.text_colour,
            &original_accu.text_colour,
            &style.original_style.primary_colour,
        );
        compact!(border_colour);
        compact!(shadow_colour);

        self.compact_transparency(local, original_accu, style);

        #[expect(
            clippy::float_cmp,
            reason = "exact comparisons necessary to only omit the override tag when it would be exactly the same"
        )]
        let new_drawing_baseline_offset = (self.drawing_baseline_offset
            != original_accu.drawing_baseline_offset)
            .then_some(self.drawing_baseline_offset);
        local.drawing_baseline_offset = new_drawing_baseline_offset;

        local.animations.clear();
    }

    // Transparency needs special handling since the fade needs to be applied in each case.
    fn compact_transparency(
        &mut self,
        local: &mut Local,
        original_accu: &RenderContext,
        style: StyleContext,
    ) {
        let text_transparency = if self.use_secondary {
            self.secondary_transparency
        } else {
            self.primary_transparency
        };

        self.text_transparency_after_fade = text_transparency;
        apply_fade(&mut self.text_transparency_after_fade, self.fade_value);
        local.primary_transparency = compact(
            &self.text_transparency_after_fade,
            &original_accu.text_transparency_after_fade,
            &style.original_style.primary_transparency,
        );

        // We need to add the secondary transparency as well,
        // since libass uses it for determining border fill state
        // https://github.com/libass/libass/blob/c425f6d7ec9ca7e5dfa3f8bbed29a6ddbf39a596/libass/ass_render.c#L2510
        local.secondary_transparency = compact(
            &self.secondary_transparency,
            &original_accu.secondary_transparency,
            &style.original_style.secondary_transparency,
        );

        self.border_transparency_after_fade = self.border_transparency;
        apply_fade(&mut self.border_transparency_after_fade, self.fade_value);
        local.border_transparency = compact(
            &self.border_transparency_after_fade,
            &original_accu.border_transparency_after_fade,
            &style.original_style.border_transparency,
        );
        self.shadow_transparency_after_fade = self.shadow_transparency;
        apply_fade(&mut self.shadow_transparency_after_fade, self.fade_value);
        local.shadow_transparency = compact(
            &self.shadow_transparency_after_fade,
            &original_accu.shadow_transparency_after_fade,
            &style.original_style.shadow_transparency,
        );
    }

    fn apply_karaoke(
        &mut self,
        time: TimeContext,
        effect_data: (subtitle::Duration, Option<KaraokeEffect>),
    ) {
        let (duration, effect) = effect_data;
        match effect {
            None => self.use_secondary = false,
            Some(KaraokeEffect::FillInstant) => {
                self.use_secondary = time.relative() < duration;
            }
            _ => {
                // Not yet supported
                // TODO: we might be able to support at least `\ko` by inserting karaoke
                // override tags into the output: `a{\kt214748364.7\ko0}b` guarantees
                // the `ko` effect for the `b` syllable. Maybe a similar technique could
                // even work for `\kt`.
                self.use_secondary = false;
            }
        }
    }

    /// Determine whether the changes made in this render context
    /// compared to the given previous one would start a new run in libass.
    /// Roughly corresponds to libass' `split_style_runs`.
    fn starts_new_run(&self, previous: &RenderContext) -> RespanState {
        // Missing: font->desc.vertical; border_style
        #[expect(clippy::float_cmp, reason = "to exactly match libass")]
        if self.font_name != previous.font_name
            || self.font_size != previous.font_size
            || self.primary_colour != previous.primary_colour
            || self.secondary_colour != previous.secondary_colour
            || self.border_colour != previous.border_colour
            || self.shadow_colour != previous.shadow_colour
            || self.primary_transparency != previous.primary_transparency
            || self.secondary_transparency != previous.secondary_transparency
            || self.border_transparency != previous.border_transparency
            || self.shadow_transparency != previous.shadow_transparency
            || self.soften != previous.soften
            || self.gaussian_blur != previous.gaussian_blur
            || self.shadow != previous.shadow
            || self.text_rotation != previous.text_rotation
            || self.text_shear != previous.text_shear
            || self.border != previous.border
            || self.letter_spacing != previous.letter_spacing
            || self.italic != previous.italic
            || self.font_weight != previous.font_weight
            || self.underline != previous.underline
            || self.strike_out != previous.strike_out
        {
            RespanState::StartNewRun
        } else {
            RespanState::Default
        }
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
            text_colour: Colour::BLACK,
            text_transparency_after_fade: Transparency(0),
            border_transparency_after_fade: Transparency(0),
            shadow_transparency_after_fade: Transparency(0),
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

/// Determine the value that a tag will have after applying the given `Resettable`
/// in its context.
fn apply_resettable<T>(
    original_value: &mut T,
    target_value: &Resettable<T>,
    current_style_value: &T,
) where
    T: Clone + PartialEq,
{
    match *target_value {
        Resettable::Keep => {
            // Do not change the previous value.
        }
        Resettable::Reset => {
            // Values are always reset to the current style, not to the original style.
            // So `{\rNewStyle\b1\b}` would reset the style to the bold value of `NewStyle`.
            *original_value = current_style_value.clone();
        }
        Resettable::Override(ref override_value) => *original_value = override_value.clone(),
    }
}

/// Similar to `apply_resettable`, but animated.
fn animate_single<T>(
    original_value: &mut T,
    target_value: Resettable<T>,
    current_style_value: T,
    power: f64,
) where
    T: Lerp<Output = T> + Copy,
{
    match target_value {
        Resettable::Keep => {}
        Resettable::Reset => *original_value = current_style_value,
        Resettable::Override(override_value) => {
            *original_value = original_value.lerp(override_value, power);
        }
    }
}

fn animate_soften(
    original_value: &mut i32,
    target_value: Resettable<i32>,
    current_style_value: i32,
    power: f64,
) {
    match target_value {
        Resettable::Keep => {}
        Resettable::Reset => *original_value = current_style_value,
        Resettable::Override(override_value) => {
            let new_soften = f64::from(*original_value).lerp(f64::from(override_value), power);
            *original_value = ass_dtoi32(new_soften + 0.5);
        }
    }
}

fn animate_font_size(
    original_value: &mut f64,
    target_value: FontSize,
    current_style_value: f64,
    power: f64,
) {
    match target_value {
        FontSize::Delta(delta) => {
            *original_value =
                apply_font_size_delta(*original_value, delta, current_style_value, power);
        }
        FontSize::Reset(delta) => {
            *original_value =
                apply_font_size_delta(current_style_value, delta, current_style_value, power);
        }
        FontSize::Set(font_size) => {
            let val = original_value.lerp(font_size, power);
            *original_value = if val <= 0.0 { current_style_value } else { val }
        }
    }
}

fn apply_font_size_delta(
    original_value: f64,
    delta: FontSizeDelta,
    current_style_value: f64,
    power: f64,
) -> f64 {
    // +10 corresponds to a doubling of font size.
    let val = original_value * (1.0 + power * delta.0 / 10.0);
    if val <= 0.0 { current_style_value } else { val }
}

/// Finds a compact `Resettable` representation of the given value in its context.
fn compact<T>(value: &T, previous_value: &T, original_style_value: &T) -> Resettable<T>
where
    T: Clone + PartialEq,
{
    if value == previous_value {
        // We just set the value to whatever we had previously,
        // so we can ignore it.
        Resettable::Keep
    } else if value == original_style_value {
        // Similar idea, but we can reset it to the original style.
        // Note that the result of baking should be an event that ONLY depends on the original style
        // and does not require any style lookup logic to further process. So we cannot depend
        // on the current style value and can only ever reset to the original style value.
        Resettable::Reset
    } else {
        // True override
        Resettable::Override(value.clone())
    }
}

fn compact_font_size(value: f64, previous_value: f64, original_style_value: f64) -> FontSize {
    #[expect(
        clippy::float_cmp,
        reason = "exact comparisons necessary to only omit the override tag when it would be exactly the same"
    )]
    let font_size = if value == previous_value {
        FontSize::KEEP
    } else if value == original_style_value {
        FontSize::Reset(FontSizeDelta::ZERO)
    } else {
        FontSize::Set(value)
    };
    font_size
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
        let numerator = f64::from((now.cast_unsigned() - t1.cast_unsigned()).cast_signed());
        let denominator = f64::from(t2.cast_unsigned() - t1.cast_unsigned());
        (numerator / denominator).powf(acceleration)
    }
}

fn has_karaoke(spans: &[Span]) -> bool {
    for span in spans {
        match *span {
            Span::Tags(ref local, _) | Span::Drawing(ref local, _)
                if local.karaoke.effect.is_some() =>
            {
                return true;
            }

            _ => {}
        }
    }

    false
}

#[derive(Debug, Clone, Copy, Default)]
enum RespanState {
    #[default]
    Default,

    /// Force start a new run even if the karaoke duration is zero.
    StartNewRun,
}

/// Preprocess and split spans for correct karaoke purposes.
///
/// In libass, each new run (`starts_new_run`) automatically becomes a new karaoke syl.
/// Our karaoke logic, however, assumes karaoke syls are only those with nonzero duration.
/// So we need to split up spans containing multiple runs (i.e. those with linebreaks)
/// and mark spans that start a new run (due to style changes) even without a specified
/// karaoke duration.
fn respan<'a, F: Fn(&str) -> Option<&'a subtitle::Style>>(
    time: TimeContext,
    mut style_context: StyleContext<'a>,
    style_lookup: &'a F,
    spans: &[Span],
    global_overrides_option: Option<&Local>,
) -> (Vec<Span>, Vec<RespanState>) {
    let mut new_spans = Vec::with_capacity(spans.len());
    let mut respan_states = Vec::with_capacity(spans.len());

    // Essentially we just run a stripped-down version of the baking process
    // in order to be able to detect style changes.
    let mut accu = RenderContext::default();
    accu.reset(style_context.original_style);

    for span in spans {
        match *span {
            Span::Tags(ref local, ref text) => {
                let mut local_original = Some(local.clone());
                let mut local_copy = local.clone();

                let respan_state = bake_local(
                    time,
                    style_context,
                    &mut accu,
                    &mut local_copy,
                    global_overrides_option,
                );

                let mut respan_state_opt = Some(respan_state);
                let mut add_newline = false;

                // This does not match libass behaviour, as libass can use libunibreak to break at
                // exotic newline characters.
                // Also, libass also starts a new run for automatically broken lines...
                for run in text.split("\\N") {
                    // Use the original tags and the determined respan state for the first span.
                    // For all subsequent newly created spans, use empty tags and force-start a new run.

                    // TODO: possible small optimization: a karaoke effect can only ever
                    // apply to the second span after it is specified, not any subsequent ones.
                    // So we might not need to make a new span for *every* new line.
                    let new_local = local_original.take().unwrap_or_default();
                    let new_run = if add_newline {
                        format!("\\N{run}")
                    } else {
                        run.to_owned()
                    };
                    new_spans.push(Span::Tags(new_local, new_run));
                    respan_states.push(respan_state_opt.take().unwrap_or(RespanState::StartNewRun));

                    add_newline = true;
                }
            }
            Span::Drawing(ref local, ref drawing) => {
                let mut local_copy = local.clone();
                let respan_state = bake_local(
                    time,
                    style_context,
                    &mut accu,
                    &mut local_copy,
                    global_overrides_option,
                );

                new_spans.push(Span::Drawing(local.clone(), drawing.clone()));
                respan_states.push(respan_state);
            }
            Span::Reset => {
                style_context.new_style = style_context.original_style;
                let (_, respan_state) =
                    bake_reset(time, style_context, &mut accu, global_overrides_option);
                new_spans.push(Span::Reset);
                respan_states.push(respan_state);
            }
            Span::ResetToStyle(ref style_name) => {
                style_context.new_style =
                    style_lookup(style_name).unwrap_or(style_context.original_style);
                let (_, respan_state) =
                    bake_reset(time, style_context, &mut accu, global_overrides_option);
                new_spans.push(Span::ResetToStyle(style_name.to_owned()));
                respan_states.push(respan_state);
            }
        }
    }

    (new_spans, respan_states)
}

/// Convert spans potentially containing karaoke elements into a vec of
/// `(transition_time, effect)` pairs, one per span.
///
/// `transition_time` is a millisecond offset from event start: before this time the span
/// shows in secondary colour; at or after it the span shows in primary.
///
/// `effect` is `None` until the first `\k`/`\kf`/`\ko` tag is encountered, then carries
/// the most recently seen effect type forward (including across `\kt`-only spans, because
/// `\kt` alone does not update `effect_type` in libass).
///
/// ## How libass represents karaoke timing
///
/// During parsing (`ass_parse_tags`), each `\k`/`\kf`/`\ko` tag updates:
/// - `state->effect_skip_timing += (uint32_t)state->effect_timing` — accumulate
///   the previous duration as "skip time" before this new syllable.
/// - `state->effect_timing = dtoi32(val * 10)` — set the new syllable duration (ms).
///
/// `\kt` instead *sets* `effect_skip_timing` absolutely and marks `reset_effect`.
///
/// Crucially, `ass_render.c:2192-2195` resets `effect_timing`, `effect_skip_timing`,
/// and `reset_effect` to 0/false after each glyph is created. So these values never
/// carry between spans; each span's first glyph gets only the tags from its own block.
///
/// ## `ass_process_karaoke_effects` (the second pass)
///
/// After all glyphs are created, `split_style_runs` decides which glyphs start a new
/// "run" (a contiguous group rendered identically). The primary trigger for a new run
/// is `effect_timing != 0`. A span with `effect_timing == 0` (e.g. `\k0` or `\kt`
/// alone) therefore does *not* start its own run — its glyph falls into the preceding
/// run and is processed in the within-run accumulation loop.
///
/// The main loop then maintains a running `timing` counter (milliseconds). For each
/// run boundary it computes:
/// ```text
/// tm_start = timing + start->effect_skip_timing
/// tm_end   = tm_start + start->effect_timing
/// timing   = !has_reset * tm_end + skip_timing   (where skip_timing accumulates
///                                                  effect_skip_timing of within-run glyphs)
/// ```
/// All glyphs in the run transition secondary→primary at `tm_start`.
///
/// ## Mapping to our span model
///
/// Because state resets after each glyph, our `Karaoke` struct encodes per-span:
/// - `NoDelay`          → `effect_skip_timing = 0`,            `reset_effect = false`
/// - `RelativeDelay(d)` → `effect_skip_timing = dtoi32(d*10)`, `reset_effect = false`
/// - `Absolute(d)`      → `effect_skip_timing = dtoi32(d*10)`, `reset_effect = true`
///
/// A span with `effect_timing == 0` shares its run with the previous boundary glyph,
/// so the returned baked value is the same as the previous span's — the span
/// "appears" to transition at the same time even though the timeline advances by `skip`.
fn bake_karaoke(
    spans: &[Span],
    respan_states: &[RespanState],
) -> Vec<(subtitle::Duration, Option<KaraokeEffect>)> {
    // Running karaoke timeline position in milliseconds. Corresponds to `timing` in
    // `ass_process_karaoke_effects` (there it's `int32_t`, but we use `i64` to avoid
    // worrying about overflow in intermediate calculations).
    let mut timing: i64 = 0;

    // The most recent baked value from a span with non-zero duration. Zero-duration
    // spans inherit this value — they fall into the previous run in libass and therefore
    // share its transition time.
    let mut last_baked = subtitle::Duration(0);

    // The most recently seen karaoke effect type. Corresponds to the accumulated
    // `effect_type` in `ass_process_karaoke_effects`. None = EF_NONE (no karaoke yet).
    // \kt alone (effect = None) does not update this, matching libass behaviour where
    // \kt leaves the glyph's effect_type at EF_NONE so the accumulated value is kept.
    let mut active_effect_type: Option<KaraokeEffect> = None;

    spans
        .iter()
        .zip(respan_states)
        .map(|(span, respan_state)| {
            let karaoke = match *span {
                Span::Tags(ref local, _) | Span::Drawing(ref local, _) => local.karaoke,
                // \r does not touch karaoke state; treat as a zero-duration no-op.
                Span::Reset | Span::ResetToStyle(_) => {
                    return (last_baked, active_effect_type);
                }
            };

            // \k / \kf / \ko set effect_type on the glyph; \kt alone leaves it EF_NONE
            // and therefore does not update active_effect_type.
            if let Some((et, _)) = karaoke.effect {
                active_effect_type = Some(et);
            }

            // Until the first \k/\kf/\ko tag the span always renders in primary colour.
            if active_effect_type.is_none() {
                return (subtitle::Duration(0), None);
            }

            // Translate our onset to libass's `effect_skip_timing` and `reset_effect`.
            let (skip, is_reset) = match karaoke.onset {
                // \k/\kf/\ko with no preceding sibling tag in the same block.
                KaraokeOnset::NoDelay => (0_i64, false),
                // Multiple \k-family tags in one block: the earlier tags' durations
                // accumulate as skip timing for the final one.
                KaraokeOnset::RelativeDelay(cs) => (i64::from(ass_dtoi32(cs.0 * 10.0)), false),
                // \kt: sets skip absolutely and marks reset_effect = true, which causes
                // `ass_process_karaoke_effects` to set timing = 0 before adding the skip.
                KaraokeOnset::Absolute(cs) => (i64::from(ass_dtoi32(cs.0 * 10.0)), true),
            };

            // effect = None means \kt was the last (or only) tag — effect_timing = 0.
            let duration = karaoke
                .effect
                .map_or(0_i64, |(_, cs)| i64::from(ass_dtoi32(cs.0 * 10.0)));

            // \kt (reset_effect = true) causes timing = 0 before applying skip.
            // In libass this happens when the within-run glyph with reset_effect is seen
            // before the next boundary; we apply it here at the span level.
            if is_reset {
                timing = 0;
            }

            // tm_start: when this span's karaoke effect begins (ms from event start).
            // tm_end: when the effect window closes; timing advances to here.
            // For non-KF effects libass also sets tm_end = tm_start for the *rendering*
            // step, but `timing` is updated from the original tm_end beforehand.
            let tm_start = timing + skip;
            let tm_end = tm_start + duration;
            timing = tm_end;

            // A span with duration == 0 does not create its own run boundary in libass
            // (because `effect_timing == 0` is the primary starts_new_run trigger).
            // !! UNLESS we need to force start a new run due to the respan state !!

            // Its glyphs end up inside the previous run and share its transition time.
            // We model this by returning last_baked unchanged; the `skip` still advanced
            // `timing` so subsequent spans are positioned correctly.
            if duration != 0 || matches!(respan_state, RespanState::StartNewRun) {
                last_baked = subtitle::Duration(tm_start);
            }
            (last_baked, active_effect_type)
        })
        .collect()
}

fn ass_dtoi32(val: f64) -> i32 {
    if val.is_nan() || val <= f64::from(i32::MIN) || val >= f64::from(i32::MAX) + 1.0 {
        i32::MIN
    } else {
        val as i32
    }
}

fn bake_global_animations(
    time: TimeContext,
    global: &mut Global,
    playback_resolution: subtitle::Resolution,
) {
    let frame_rect = Rectangle {
        x1: 0,
        y1: 0,
        x2: playback_resolution.x,
        y2: playback_resolution.y,
    };

    // The rectangle clip is the only thing that can be globally animated.
    let mut accu = if let Some(ref clip) = global.rectangle_clip {
        *clip.value()
    } else {
        frame_rect
    };

    let mut last_clip: Option<Clip<Rectangle>> = None;

    for animation in global.animations.drain(..) {
        let power = calculate_power(time, animation.acceleration, animation.interval);
        if let Some(clip) = animation.modifiers.clip {
            let target = clip.value();
            accu.x1 = accu.x1.lerp(target.x1, power);
            accu.y1 = accu.y1.lerp(target.y1, power);
            accu.x2 = accu.x2.lerp(target.x2, power);
            accu.y2 = accu.y2.lerp(target.y2, power);
            last_clip = Some(clip);
        }
    }

    // Copy over clip type (inverse/contained)
    if let Some(ref new_clip) = last_clip {
        global.rectangle_clip = Some(new_clip.clone());
    }

    // Copy over clip bounds
    if let Some(ref mut clip) = global.rectangle_clip {
        let bounds = clip.value_mut();
        *bounds = accu;
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

const FORCE_WRAP_FADE: Fade = Fade::Complex(ComplexFade {
    transparency_before: DecimalTransparency(-256),
    transparency_main: DecimalTransparency(-256),
    transparency_after: DecimalTransparency(-256),
    fade_in_start: Milliseconds(0),
    fade_in_end: Milliseconds(0),
    fade_out_start: Milliseconds(0),
    fade_out_end: Milliseconds(0),
});

fn bake_move(time: TimeContext, global: &mut Global) {
    if let Some(PositionOrMove::Move(move_data)) = global.position {
        let (t1, t2) = match move_data.timing {
            Some(timing) => (timing.start_time.0, timing.end_time.0),
            None => (0, time.duration.0 as i32),
        };

        let now = time.relative().0 as i32;

        let power = if now <= t1 {
            0.0
        } else if now >= t2 {
            1.0
        } else {
            let numerator = f64::from((now.cast_unsigned() - t1.cast_unsigned()).cast_signed());
            let delta_t = f64::from(t2.cast_unsigned() - t1.cast_unsigned());
            numerator / delta_t
        };

        let new_x = power * (move_data.final_position.x - move_data.initial_position.x)
            + move_data.initial_position.x;
        let new_y = power * (move_data.final_position.y - move_data.initial_position.y)
            + move_data.initial_position.y;

        global.position = Some(PositionOrMove::Position(Position { x: new_x, y: new_y }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nde::tags::{
        parse, Centiseconds, GlobalAnimatable, Karaoke, Maybe3D, Move, MoveTiming,
    };
    use assert_float_eq::assert_float_absolute_eq;
    use assert_matches2::assert_matches;
    use std::cell::RefCell;

    #[test]
    fn local() {
        let animated_tags = LocalAnimatable {
            letter_spacing: Resettable::Override(3.0),
            ..LocalAnimatable::default()
        };

        let tags = Local {
            italic: Resettable::Override(true),
            letter_spacing: Resettable::Override(2.0),
            border_transparency: Resettable::Reset,
            animations: vec![Animation {
                modifiers: animated_tags,
                acceleration: 1.0,
                interval: None,
            }],
            ..Local::empty()
        };

        let original_style = subtitle::Style {
            border_transparency: Transparency(100),
            ..subtitle::Style::default()
        };
        let new_style = subtitle::Style {
            border_transparency: Transparency(50),
            ..subtitle::Style::default()
        };
        let style = StyleContext {
            original_style: &original_style,
            new_style: &new_style,
        };
        let mut accu = RenderContext::default();
        accu.reset(style.original_style);

        let mut time = TimeContext {
            start: subtitle::StartTime(1000),
            duration: subtitle::Duration(3000),
            now: subtitle::StartTime(1000),
        };

        let mut new_accu = accu.clone();
        let mut new_local = tags.clone();
        bake_local(time, style, &mut new_accu, &mut new_local, None);
        assert_matches!(new_local.italic, Resettable::Override(true));
        assert_matches!(new_local.underline, Resettable::Keep);
        assert_matches!(new_local.letter_spacing, Resettable::Override(fsp));
        assert_float_absolute_eq!(fsp, 2.0, 0.01);
        // We cannot depend on the new style in the bake result.
        // So this must result in an override, not in a reset.
        assert_matches!(
            new_local.border_transparency,
            Resettable::Override(Transparency(50))
        );
        assert!(new_local.animations.is_empty());

        let mut new_accu = accu.clone();
        new_accu.reset(&new_style);
        let mut new_local = tags;
        new_local.border_transparency = Resettable::Override(Transparency(100));
        time.now = subtitle::StartTime(2500);
        bake_local(time, style, &mut new_accu, &mut new_local, None);
        assert_matches!(new_local.letter_spacing, Resettable::Override(fsp));
        assert_float_absolute_eq!(fsp, 2.5, 0.01);
        assert_matches!(new_local.border_transparency, Resettable::Reset);
    }

    #[test]
    fn animation() {
        let mut time = TimeContext {
            start: subtitle::StartTime(1000),
            duration: subtitle::Duration(3000),
            now: subtitle::StartTime(0),
        };

        assert_float_absolute_eq!(calculate_power(time, 1.0, None), 0.0, 0.01);
        time.now = subtitle::StartTime(1000);
        assert_float_absolute_eq!(calculate_power(time, 1.0, None), 0.0, 0.01);
        time.now = subtitle::StartTime(2000);
        assert_float_absolute_eq!(calculate_power(time, 1.0, None), 0.33, 0.01);
        time.now = subtitle::StartTime(3000);
        assert_float_absolute_eq!(calculate_power(time, 1.0, None), 0.67, 0.01);
        assert_float_absolute_eq!(calculate_power(time, 2.0, None), 0.44, 0.01);
        time.now = subtitle::StartTime(4000);
        assert_float_absolute_eq!(calculate_power(time, 1.0, None), 1.0, 0.01);
        time.now = subtitle::StartTime(5000);
        assert_float_absolute_eq!(calculate_power(time, 1.0, None), 1.0, 0.01);

        let original_style = subtitle::Style::default();
        let new_style = subtitle::Style {
            angle: subtitle::Angle(100.0),
            ..subtitle::Style::default()
        };
        let style = StyleContext {
            original_style: &original_style,
            new_style: &new_style,
        };
        let mut accu = RenderContext::default();
        accu.reset(style.original_style);
        accu.text_rotation.x = 50.0;
        accu.text_rotation.y = 70.0;
        accu.text_rotation.z = 90.0;

        let tags = LocalAnimatable {
            text_rotation: Maybe3D {
                x: Resettable::Keep,
                y: Resettable::Override(60.0),
                z: Resettable::Reset,
            },
            ..LocalAnimatable::default()
        };

        let animations = &[Animation {
            modifiers: tags,
            acceleration: 1.0,
            interval: Some(AnimationInterval {
                start: Milliseconds(500),
                end: Milliseconds(1000),
            }),
        }];

        time.now = subtitle::StartTime(1500);
        let mut new_accu = accu.clone();
        new_accu.animate(time, style, animations);
        assert_float_absolute_eq!(new_accu.text_rotation.x, 50.0, 0.01);
        assert_float_absolute_eq!(new_accu.text_rotation.y, 70.0, 0.01);
        assert_float_absolute_eq!(new_accu.text_rotation.z, 100.0, 0.01);

        time.now = subtitle::StartTime(1750);
        let mut new_accu = accu.clone();
        new_accu.animate(time, style, animations);
        assert_float_absolute_eq!(new_accu.text_rotation.x, 50.0, 0.01);
        assert_float_absolute_eq!(new_accu.text_rotation.y, 65.0, 0.01);
        assert_float_absolute_eq!(new_accu.text_rotation.z, 100.0, 0.01);

        time.now = subtitle::StartTime(2000);
        let mut new_accu = accu.clone();
        new_accu.animate(time, style, animations);
        assert_float_absolute_eq!(new_accu.text_rotation.x, 50.0, 0.01);
        assert_float_absolute_eq!(new_accu.text_rotation.y, 60.0, 0.01);
        assert_float_absolute_eq!(new_accu.text_rotation.z, 100.0, 0.01);
    }

    #[test]
    fn compact() {
        let original_style = subtitle::Style::default();
        let new_style = subtitle::Style::default();
        let style_context = StyleContext {
            original_style: &original_style,
            new_style: &new_style,
        };

        let mut accu = RenderContext::default();
        accu.reset(style_context.original_style);
        accu.strike_out = true;

        let mut new_accu = accu.clone();
        new_accu.italic = true;
        new_accu.strike_out = false;

        let mut local = Local::empty();

        new_accu.compact_all(&mut local, &accu, style_context);

        assert_matches!(local.underline, Resettable::Keep);
        assert_matches!(local.strike_out, Resettable::Reset);
        assert_matches!(local.italic, Resettable::Override(true));
    }

    fn karaoke_span(karaoke: Karaoke) -> Span {
        Span::Tags(
            Local {
                karaoke,
                ..Local::empty()
            },
            String::new(),
        )
    }

    #[test]
    fn karaoke() {
        let spans = vec![
            karaoke_span(Karaoke::empty()),
            karaoke_span(Karaoke {
                effect: Some((KaraokeEffect::FillInstant, Centiseconds(20.0))),
                onset: KaraokeOnset::NoDelay,
            }),
            karaoke_span(Karaoke {
                effect: Some((KaraokeEffect::BorderInstant, Centiseconds(30.0))),
                onset: KaraokeOnset::NoDelay,
            }),
            karaoke_span(Karaoke {
                effect: Some((KaraokeEffect::FillInstant, Centiseconds(0.0))),
                onset: KaraokeOnset::RelativeDelay(Centiseconds(40.0)),
            }),
            karaoke_span(Karaoke {
                effect: Some((KaraokeEffect::FillInstant, Centiseconds(50.0))),
                onset: KaraokeOnset::NoDelay,
            }),
            karaoke_span(Karaoke {
                effect: Some((KaraokeEffect::FillInstant, Centiseconds(300.0))),
                onset: KaraokeOnset::NoDelay,
            }),
            karaoke_span(Karaoke {
                effect: None,
                onset: KaraokeOnset::Absolute(Centiseconds(200.0)),
            }),
            karaoke_span(Karaoke {
                effect: Some((KaraokeEffect::FillInstant, Centiseconds(50.0))),
                onset: KaraokeOnset::NoDelay,
            }),
            karaoke_span(Karaoke {
                effect: Some((KaraokeEffect::FillInstant, Centiseconds(0.0))),
                onset: KaraokeOnset::NoDelay,
            }),
            karaoke_span(Karaoke {
                effect: Some((KaraokeEffect::FillInstant, Centiseconds(-50.0))),
                onset: KaraokeOnset::NoDelay,
            }),
            karaoke_span(Karaoke {
                effect: Some((KaraokeEffect::FillInstant, Centiseconds(-100.0))),
                onset: KaraokeOnset::NoDelay,
            }),
        ];

        let (_, parsed_spans) = parse(
            "a{\\k20}a{\\ko30}a{\\k40\\k0}a{\\k50}a{\\k300}a{\\kt200}a{\\k50}a{\\k0}a{\\k-50}a{\\k-100}a",
        );
        for (i, span) in spans.iter().enumerate() {
            assert_matches!(span, &Span::Tags(ref specified_tags, _));
            assert_matches!(&parsed_spans[i], &Span::Tags(ref parsed_tags, _));
            assert_eq!(specified_tags.karaoke, parsed_tags.karaoke);
        }

        let respan_states: Vec<RespanState> =
            std::iter::repeat_n(RespanState::Default, spans.len()).collect();
        let baked = bake_karaoke(&spans, &respan_states);

        assert_eq!(baked[0], (subtitle::Duration(0), None));
        assert_eq!(
            baked[1],
            (subtitle::Duration(0), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[2],
            (subtitle::Duration(200), Some(KaraokeEffect::BorderInstant))
        );
        assert_eq!(
            baked[3],
            (subtitle::Duration(200), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[4],
            (subtitle::Duration(900), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[5],
            (subtitle::Duration(1400), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[6],
            (subtitle::Duration(1400), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[7],
            (subtitle::Duration(2000), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[8],
            (subtitle::Duration(2000), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[9],
            (subtitle::Duration(2500), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[10],
            (subtitle::Duration(2000), Some(KaraokeEffect::FillInstant))
        );
    }

    #[test]
    fn karaoke_local_mix() {
        let (_, spans) = parse("{\\k20}a{\\k20}b{}c{\\k20}d\\Ne{\\k20}f{\\3c&HFFFF00&}g");
        assert_eq!(spans.len(), 6);

        let time = TimeContext {
            start: subtitle::StartTime(1000),
            duration: subtitle::Duration(3000),
            now: subtitle::StartTime(2000),
        };

        let event_style = subtitle::Style::default();

        let style_context = StyleContext {
            original_style: &event_style,
            new_style: &event_style,
        };

        let style_lookup = |_name: &str| panic!("the style lookup should not have been called");

        let (new_spans, respan_state) = respan(time, style_context, &style_lookup, &spans, None);
        assert_eq!(new_spans.len(), 7);
        assert_eq!(respan_state.len(), 7);

        assert_matches!(&new_spans[0], &Span::Tags(_, ref text));
        assert_eq!(text, "a");
        assert_matches!(&respan_state[0], &RespanState::Default);

        assert_matches!(&new_spans[1], &Span::Tags(_, ref text));
        assert_eq!(text, "b");
        assert_matches!(&respan_state[1], &RespanState::Default);

        assert_matches!(&new_spans[2], &Span::Tags(_, ref text));
        assert_eq!(text, "c");
        assert_matches!(&respan_state[2], &RespanState::Default);

        assert_matches!(&new_spans[3], &Span::Tags(_, ref text));
        assert_eq!(text, "d");
        assert_matches!(&respan_state[3], &RespanState::Default);

        assert_matches!(&new_spans[4], &Span::Tags(_, ref text));
        assert_eq!(text, "\\Ne");
        assert_matches!(&respan_state[4], &RespanState::StartNewRun);

        assert_matches!(&new_spans[5], &Span::Tags(_, ref text));
        assert_eq!(text, "f");
        assert_matches!(&respan_state[5], &RespanState::Default);

        assert_matches!(&new_spans[6], &Span::Tags(_, ref text));
        assert_eq!(text, "g");
        assert_matches!(&respan_state[6], &RespanState::StartNewRun);

        let baked = bake_karaoke(&new_spans, &respan_state);

        assert_eq!(
            baked[0],
            (subtitle::Duration(0), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[1],
            (subtitle::Duration(200), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[2],
            (subtitle::Duration(200), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[3],
            (subtitle::Duration(400), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[4],
            (subtitle::Duration(600), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[5],
            (subtitle::Duration(600), Some(KaraokeEffect::FillInstant))
        );
        assert_eq!(
            baked[6],
            (subtitle::Duration(800), Some(KaraokeEffect::FillInstant))
        );
    }

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
    fn compact_fade() {
        let original_style = subtitle::Style::default();
        let new_style = subtitle::Style::default();
        let style_context = StyleContext {
            original_style: &original_style,
            new_style: &new_style,
        };

        let accu = RenderContext {
            fade_value: Transparency(200),
            ..RenderContext::default()
        };

        let mut new_accu = accu.clone();
        new_accu.primary_transparency = Transparency(100);
        let mut local = Local::empty();

        new_accu.compact_all(&mut local, &accu, style_context);

        assert_matches!(
            local.primary_transparency,
            Resettable::Override(Transparency(222))
        );
        assert_matches!(
            local.border_transparency,
            Resettable::Override(Transparency(200))
        );
    }

    #[test]
    fn reset() {
        let time = TimeContext {
            start: subtitle::StartTime::default(),
            duration: subtitle::Duration::default(),
            now: subtitle::StartTime::default(),
        };
        let original_style = subtitle::Style {
            border_transparency: Transparency(100),
            ..subtitle::Style::default()
        };
        let new_style = subtitle::Style {
            border_transparency: Transparency(50),
            ..subtitle::Style::default()
        };
        let style = StyleContext {
            original_style: &original_style,
            new_style: &new_style,
        };
        let mut accu = RenderContext::default();
        accu.reset(style.original_style);
        let (local, _) = bake_reset(time, style, &mut accu, None);
        assert_eq!(local.underline, Resettable::Keep,);
        assert_matches!(
            local.border_transparency,
            Resettable::Override(Transparency(50))
        );
    }

    #[test]
    fn global_animation() {
        let source_rect = Rectangle {
            x1: 50,
            y1: 50,
            x2: 100,
            y2: 100,
        };

        let target_rect = Rectangle {
            x1: 100,
            y1: 50,
            x2: 125,
            y2: 100,
        };

        let global = Global {
            position: None,
            rectangle_clip: Some(Clip::Contained(source_rect)),
            vector_clip: None,
            origin: None,
            fade: None,
            wrap_style: Resettable::Keep,
            alignment: Resettable::Keep,
            animations: vec![Animation {
                modifiers: GlobalAnimatable {
                    clip: Some(Clip::Inverse(target_rect)),
                },
                acceleration: 1.0,
                interval: None,
            }],
        };

        let mut time = TimeContext {
            start: subtitle::StartTime(1000),
            duration: subtitle::Duration(3000),
            now: subtitle::StartTime(1000),
        };

        let resolution = subtitle::Resolution { x: 1920, y: 1080 };

        let mut new_global = global.clone();
        bake_global_animations(time, &mut new_global, resolution);
        assert_matches!(new_global.rectangle_clip, Some(Clip::Inverse(rect)));
        assert_eq!(rect.x1, 50);

        time.now = subtitle::StartTime(2500);
        let mut new_global = global.clone();
        bake_global_animations(time, &mut new_global, resolution);
        assert_matches!(new_global.rectangle_clip, Some(Clip::Inverse(rect)));
        assert_eq!(rect.x1, 75);

        time.now = subtitle::StartTime(4000);
        let mut new_global = global;
        bake_global_animations(time, &mut new_global, resolution);
        assert_matches!(new_global.rectangle_clip, Some(Clip::Inverse(rect)));
        assert_eq!(rect.x1, 100);
    }

    #[test]
    fn global_move() {
        let global = Global {
            position: Some(PositionOrMove::Move(Move {
                initial_position: Position { x: 10.0, y: 20.0 },
                final_position: Position { x: 30.0, y: 60.0 },
                timing: Some(MoveTiming {
                    start_time: Milliseconds(500),
                    end_time: Milliseconds(1000),
                }),
            })),
            rectangle_clip: None,
            vector_clip: None,
            origin: None,
            fade: None,
            wrap_style: Resettable::Keep,
            alignment: Resettable::Keep,
            animations: vec![],
        };

        let mut time = TimeContext {
            start: subtitle::StartTime(1000),
            duration: subtitle::Duration(3000),
            now: subtitle::StartTime(1000),
        };

        let mut new_global = global.clone();
        bake_move(time, &mut new_global);
        assert_matches!(new_global.position, Some(PositionOrMove::Position(pos)));
        assert_float_absolute_eq!(pos.x, 10.0, 0.01);

        time.now = subtitle::StartTime(1500);
        let mut new_global = global.clone();
        bake_move(time, &mut new_global);
        assert_matches!(new_global.position, Some(PositionOrMove::Position(pos)));
        assert_float_absolute_eq!(pos.x, 10.0, 0.01);

        time.now = subtitle::StartTime(1750);
        let mut new_global = global.clone();
        bake_move(time, &mut new_global);
        assert_matches!(new_global.position, Some(PositionOrMove::Position(pos)));
        assert_float_absolute_eq!(pos.x, 20.0, 0.01);

        time.now = subtitle::StartTime(2000);
        let mut new_global = global.clone();
        bake_move(time, &mut new_global);
        assert_matches!(new_global.position, Some(PositionOrMove::Position(pos)));
        assert_float_absolute_eq!(pos.x, 30.0, 0.01);

        time.now = subtitle::StartTime(3000);
        let mut new_global = global;
        bake_move(time, &mut new_global);
        assert_matches!(new_global.position, Some(PositionOrMove::Position(pos)));
        assert_float_absolute_eq!(pos.x, 30.0, 0.01);
    }

    #[test]
    fn all() {
        let line = "{\\fade(50,100,150,250,750,1250,1750))\\clip(75,25,125,75)\\move(80,80,100,100)\\t(\\clip(50,0,100,50))\\k25}Sphinx {\\t(500,1500,0.5,\\3c&H00FFFF&)\\k25}of {\\rStyle 2\\4c&HFF00FF&\\k25}black\\Nquartz, judge\\Nmy vow";
        let (mut global, mut spans) = parse(line);
        assert_eq!(spans.len(), 4);

        let time = TimeContext {
            start: subtitle::StartTime(1000),
            duration: subtitle::Duration(2000),
            now: subtitle::StartTime(1700),
        };

        let event_style = subtitle::Style::default();
        let style_2 = subtitle::Style {
            border_width: 10.0,
            ..subtitle::Style::default()
        };

        let style_lookup_called_counter = RefCell::new(0);
        let style_lookup = |name: &str| {
            *style_lookup_called_counter.borrow_mut() += 1;
            if name == "Style 2" {
                Some(&style_2)
            } else {
                panic!("the style lookup should not have been called with style name: '{name}'")
            }
        };

        let resolution = subtitle::Resolution { x: 1920, y: 1080 };

        let green = Colour {
            red: 0,
            green: 255,
            blue: 0,
        };
        let global_overrides = Local {
            shadow_colour: Resettable::Override(green),
            ..Local::empty()
        };

        bake(
            time,
            &event_style,
            &style_lookup,
            &mut global,
            &mut spans,
            resolution,
            Some(&global_overrides),
        );

        // once in respan, once in bake itself
        assert_eq!(style_lookup_called_counter.take(), 2);

        // Clip
        assert_matches!(
            global.rectangle_clip,
            Some(Clip::Contained(global_rectangle_clip))
        );
        assert_eq!(global_rectangle_clip.x1, 66);
        assert!(global.animations.is_empty());

        // Position
        assert_matches!(
            global.position,
            Some(PositionOrMove::Position(global_position))
        );
        assert_float_absolute_eq!(global_position.x, 87.0, 0.01);

        assert_matches!(&spans[0], &Span::Tags(ref local_0, ref text_0));
        assert_matches!(&spans[1], &Span::Tags(ref local_1, ref text_1));
        assert_matches!(&spans[2], &Span::Tags(ref local_2, ref text_2));
        assert_matches!(&spans[3], &Span::Tags(ref local_3, ref text_3));
        assert_matches!(&spans[4], &Span::Tags(ref local_4, ref text_4));
        assert_eq!(text_0, "Sphinx ");
        assert_eq!(text_1, "of ");
        assert_eq!(text_2, "");
        assert_eq!(text_3, "black");
        assert_eq!(text_4, "\\Nquartz, judge");

        // Karaoke
        assert_matches!(local_0.primary_colour, Resettable::Keep);
        assert_matches!(local_1.primary_colour, Resettable::Keep);
        assert_matches!(local_2.primary_colour, Resettable::Keep);
        assert_matches!(local_3.primary_colour, Resettable::Keep);
        assert_matches!(
            local_4.primary_colour,
            Resettable::Override(local_4_primary_colour)
        );
        assert_eq!(local_4_primary_colour, event_style.secondary_colour);

        // Fade
        assert_matches!(global.fade, None);
        assert_matches!(
            local_0.primary_transparency,
            Resettable::Override(transparency)
        );
        assert_eq!(transparency, Transparency(95));
        assert_matches!(local_1.primary_transparency, Resettable::Keep);

        // \t animation
        assert_matches!(
            local_1.border_colour,
            Resettable::Override(local_1_border_colour)
        );
        assert_eq!(
            local_1_border_colour,
            Colour {
                red: 114,
                green: 114,
                blue: 0
            }
        );

        // Reset
        assert_matches!(local_2.border_colour, Resettable::Reset);
        assert_matches!(local_2.border.x, Resettable::Override(10.0));

        // Global override
        assert_matches!(
            local_0.shadow_colour,
            Resettable::Override(local_0_shadow_colour)
        );
        assert_eq!(local_0_shadow_colour, green);
        assert_matches!(local_3.shadow_colour, Resettable::Keep);
    }
}
