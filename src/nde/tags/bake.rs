#![allow(
    clippy::cast_possible_truncation,
    reason = "this module needs to convert lots of types back and forth to exactly match libass' behavior"
)]

use super::{
    lerp::Lerp, Animation, AnimationInterval, Colour, ComplexFade, DecimalTransparency, Fade,
    FontEncoding, FontSize, FontSizeDelta, FontWeight, Global, KaraokeEffect, KaraokeOnset,
    Local, LocalAnimatable, Milliseconds, Resettable, SimpleFade, Transparency,
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

#[derive(Debug, Clone, Copy)]
pub struct StyleContext<'a> {
    pub original_style: &'a subtitle::Style,
    pub new_style: &'a subtitle::Style,
}

/// Bakes styles and animations into the given event.
///
/// In a nutshell, this method applies the animations and style overrides present in the given event data.
/// as they would appear at the given time.
///
/// Limitations:
/// - Karaoke sweeps are not handled (`\K` / `\kf` tags; `KaraokeEffect::FillSweep`)
/// - The `\ko` / `KaraokeEffect:BorderInstant` karaoke effect is also not handled
/// - Effects (as in, marquee etc.) are not handled
pub fn bake<'a, F: Fn(&str) -> &'a subtitle::Style>(
    time: TimeContext,
    event_style: &'a subtitle::Style,
    style_lookup: &'a F,
    global_tags: &Global,
    global_overrides_option: Option<&Local>,
    spans: &mut [Span],
) {
    let style_context = StyleContext {
        original_style: event_style,
        new_style: event_style,
    };

    let mut accu = RenderContext::default();
    accu.reset(event_style);

    // Apply local overrides
    if let Some(global_overrides) = global_overrides_option {
        accu.apply_all_resettables(style_context, global_overrides);
        accu.animate(time, style_context, &global_overrides.animations);
    }

    let fade = global_tags
        .fade
        .map_or(Transparency(0), |fade| bake_fade(time, fade));

    let karaoke = bake_karaoke(spans);

    for (i, span) in spans.iter_mut().enumerate() {
        accu.apply_karaoke(time, karaoke[i]);

        match *span {
            Span::Tags(ref mut local, ref text) => {
                // Clear global overrides from local tags, since they have already been
                // applied to the accumulator.
                if let Some(global_overrides) = global_overrides_option {
                    local.clear_from(global_overrides);
                }
            }
            Span::Reset => {}
            Span::ResetToStyle(ref style_name) => {}
            Span::Drawing(ref mut local, ref drawing) => {
                if let Some(global_overrides) = global_overrides_option {
                    local.clear_from(global_overrides);
                }
            }
        }
    }
}

fn bake_local(time: TimeContext, style: StyleContext, accu: &mut RenderContext, local: &mut Local) {
    // First, we make a copy of the original render context, so we can compare the
    // changes that were made by the local tags.
    let original_accu = accu.clone();

    // Then, we apply the static resettable-style override tags to the render context,
    // updating all property values that are supposed to be changed.
    // This does not yet handle animations.
    accu.apply_all_resettables(style, local);

    // Now, we apply all animations in order.
    accu.animate(time, style, &local.animations);

    // Finally, we take the difference between the changed render context and the
    // original one, and convert this difference into new override tags.
    accu.compact_all(local, &original_accu, style);
}

fn bake_reset(style: StyleContext, accu: &mut RenderContext) -> Local {
    // This method is similar to `bake_local`, except we reset the render context
    // to `new_style`.
    let original_accu = accu.clone();
    accu.reset(style.new_style);
    let mut local = Local::empty();
    accu.compact_all(&mut local, &original_accu, style);
    local
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

            animate_single!(soften, 0);
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

    fn compact_all(&self, local: &mut Local, original_accu: &RenderContext, style: StyleContext) {
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
        let (colour, original_colour) = if self.use_secondary {
            (self.secondary_colour, original_accu.secondary_colour)
        } else {
            (self.primary_colour, original_accu.primary_colour)
        };

        local.primary_colour = compact(
            &colour,
            &original_colour,
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
        &self,
        local: &mut Local,
        original_accu: &RenderContext,
        style: StyleContext,
    ) {
        let (transparency, original_transparency) = if self.use_secondary {
            (
                self.secondary_transparency,
                original_accu.secondary_transparency,
            )
        } else {
            (
                self.primary_transparency,
                original_accu.primary_transparency,
            )
        };

        let mut primary_transparency = transparency;
        apply_fade(&mut primary_transparency, self.fade_value);
        local.primary_transparency = compact(
            &primary_transparency,
            &original_transparency,
            &style.original_style.primary_transparency,
        );
        let mut border_transparency = self.border_transparency;
        apply_fade(&mut border_transparency, self.fade_value);
        local.border_transparency = compact(
            &border_transparency,
            &original_accu.border_transparency,
            &style.original_style.border_transparency,
        );
        let mut shadow_transparency = self.shadow_transparency;
        apply_fade(&mut shadow_transparency, self.fade_value);
        local.shadow_transparency = compact(
            &shadow_transparency,
            &original_accu.shadow_transparency,
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
fn bake_karaoke(spans: &[Span]) -> Vec<(subtitle::Duration, Option<KaraokeEffect>)> {
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
        .map(|span| {
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
            // Its glyphs end up inside the previous run and share its transition time.
            // We model this by returning last_baked unchanged; the `skip` still advanced
            // `timing` so subsequent spans are positioned correctly.
            if duration != 0 {
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

fn bake_global_animations() {
    // TODO
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

fn bake_move() {
    // TODO
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nde::tags::{parse, Centiseconds, Karaoke, Maybe3D};
    use assert_float_eq::assert_float_absolute_eq;
    use assert_matches2::assert_matches;

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
        bake_local(time, style, &mut new_accu, &mut new_local);
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
        let mut new_local = tags;
        new_local.border_transparency = Resettable::Override(Transparency(100));
        time.now = subtitle::StartTime(2500);
        bake_local(time, style, &mut new_accu, &mut new_local);
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

        let baked = bake_karaoke(&spans);

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
}
