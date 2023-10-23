use crate::nde::Span;

use super::{
    Alignment, Animation, AnimationInterval, Centiseconds, Clip, Colour, ComplexFade, Drawing,
    Fade, FontSize, FontSizeDelta, FontWeight, Global, GlobalAnimatable, HorizontalAlignment,
    KaraokeEffect, Local, LocalAnimatable, Maybe2D, Milliseconds, Move, MoveTiming, Position,
    PositionOrMove, Rectangle, Resettable, SimpleFade, Transparency, VerticalAlignment, WrapStyle,
};

#[must_use]
pub fn parse(text: &str) -> (Box<Global>, Vec<Span>) {
    let (global, spans) = raw(text);
    let simplified = simplify(spans);
    (global, simplified)
}

#[must_use]
pub fn raw(text: &str) -> (Box<Global>, Vec<Span>) {
    let mut spans: Vec<Span> = vec![];
    let mut last_local = Box::new(Local::empty());
    let mut global = Box::new(Global::empty());

    let mut drawing: Option<Drawing> = None;

    let mut slice = text;

    let mut span_text = String::new();
    let mut maybe_block_start = false;

    'outer: while !slice.is_empty() {
        if maybe_block_start {
            maybe_block_start = false;
            if let Some(block_end) = slice.find('}') {
                // We found a tag block.
                // We first need to parse it, to find out whether we must end the current drawing
                let tag_block = parse_tag_block(&slice[1..block_end], &mut global);
                let TagBlock {
                    reset,
                    new_local,
                    new_drawing_scale,
                    end_previous_drawing,
                } = tag_block;

                // Finalise and append the previous span to the list.
                // This might reset the contents of the current drawing.
                drawing = end_span(
                    &mut spans,
                    span_text,
                    last_local,
                    drawing,
                    end_previous_drawing,
                );
                span_text = String::new();
                last_local = Box::new(new_local);

                // The tags might have contained a reset, which we append now.
                // Other tags potentially following it will be appended
                // in the next iteration
                match reset {
                    Some(Reset::Reset) => spans.push(Span::Reset),
                    Some(Reset::ResetToStyle(style_name)) => {
                        spans.push(Span::ResetToStyle(style_name));
                    }
                    None => {}
                }

                // Check whether we need to set a new scale, or create a new drawing
                if let Some(new_drawing_scale) = new_drawing_scale {
                    match &mut drawing {
                        Some(existing_drawing) => {
                            existing_drawing.scale = new_drawing_scale;

                            // We would also reset the commands here,
                            // but it has already been done in `end_span`, if necessary.
                        }
                        None => {
                            // Create a new drawing
                            drawing = Some(Drawing {
                                scale: new_drawing_scale,
                                commands: String::new(),
                            });
                        }
                    }
                }

                slice = &slice[(block_end + 1)..];
            } else {
                span_text.push('{');
                slice = &slice[1..];
            }
        } else {
            let mut last_byte_index = 0;
            let mut escape = false;
            let char_iter = slice.char_indices();

            'inner: for (byte_index, next_char) in char_iter {
                last_byte_index = byte_index;

                match next_char {
                    '\\' => {
                        if escape {
                            span_text.push('\\');
                            // Keep escape true
                        } else {
                            escape = true;
                        }
                    }
                    '{' => {
                        if escape {
                            span_text.push('{');
                            escape = false;
                        } else {
                            // We might have found a tag block
                            maybe_block_start = true;
                            break 'inner;
                        }
                    }
                    char => {
                        if escape {
                            span_text.push('\\');
                            escape = false;
                        }
                        span_text.push(char);
                    }
                }
            }

            if maybe_block_start {
                slice = &slice[last_byte_index..];
            } else {
                // We're done with the entire line
                break 'outer;
            }
        }
    }

    // Finalise the last span
    end_span(
        &mut spans, span_text, last_local, drawing,
        true, // Always end a drawing at the end of the line
    );

    (global, spans)
}

fn parse_tag_block(block: &str, global: &mut Global) -> TagBlock {
    use TagBlockParseState::*;

    let mut tag_block = TagBlock {
        reset: None,
        new_local: Local::empty(),
        new_drawing_scale: None,
        end_previous_drawing: false,
    };

    let mut state = Initial;
    let mut tag_start_bytes = 0;

    for (byte_index, next_char) in block.char_indices() {
        state = match state {
            Initial => match next_char {
                '\\' => TagStart,
                _ => Comment,
            },
            Comment => match next_char {
                '\\' => {
                    parse_tag(&block[tag_start_bytes..byte_index], global, &mut tag_block);
                    TagStart
                }
                _ => Comment,
            },
            // Skip spaces between the backslash and the actual tag name
            TagStart => match next_char {
                // There are more space characters than these, but this is what libass does
                ' ' | '\t' => TagStart,
                _ => {
                    tag_start_bytes = byte_index;
                    Tag
                }
            },
            Tag => match next_char {
                '\\' => {
                    parse_tag(&block[tag_start_bytes..byte_index], global, &mut tag_block);
                    TagStart
                }
                '(' => Parenthesis,
                _ => Tag,
            },
            // We need a separate state here because a parenthesis could contain more
            // backslash-initiated tags (like in `\t`)
            Parenthesis => match next_char {
                ')' => Comment,
                _ => Parenthesis,
            },
        }
    }

    parse_tag(&block[tag_start_bytes..], global, &mut tag_block);

    tag_block
}

enum TagBlockParseState {
    Initial,
    Comment,
    TagStart,
    Tag,
    Parenthesis,
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
fn parse_tag(tag: &str, global: &mut Global, block: &mut TagBlock) -> bool {
    if tag.is_empty() {
        return false;
    }

    let paren_pos = tag.find('(');

    // Contains the name and potentially the first argument
    let first_part = &tag[0..paren_pos.unwrap_or_else(|| tag.bytes().len())];

    let mut twa = TagWithArguments {
        first_part,
        arguments: vec![],
        has_backslash_arg: false,
        tag_found: false,
    };

    if let Some(paren_pos) = paren_pos {
        parse_paren_args(&tag[(paren_pos + 1)..], &mut twa);
    }

    let local = &mut block.new_local;

    if twa.tag::<false>("xbord") {
        local.border.x = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("ybord") {
        local.border.y = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("xshad") {
        local.shadow.x = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("yshad") {
        local.shadow.y = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("fax") {
        local.text_shear.x = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("fay") {
        local.text_shear.y = resettable(twa.float_arg(0));
    } else if twa.tag::<true>("iclip") {
        parse_clip(global, &twa, Clip::Inverse, Clip::Inverse);
    } else if twa.tag::<false>("blur") {
        local.gaussian_blur = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("fscx") {
        local.font_scale.x = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("fscy") {
        local.font_scale.y = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("fsc") {
        local.font_scale = Maybe2D {
            x: Resettable::Reset,
            y: Resettable::Reset,
        }
    } else if twa.tag::<false>("fsp") {
        local.letter_spacing = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("fs") {
        local.font_size = match twa.float_arg(0) {
            Some(parsed) => {
                let str_arg = twa.string_arg(0).unwrap();
                // Only the first character is checked — `\fs+10` increases the font size by 10,
                // whereas `\fs +10` sets it to 10.
                match str_arg.chars().next().unwrap() {
                    '+' | '-' => FontSize::Delta(FontSizeDelta(parsed)),
                    _ => {
                        // libass has the additional behaviour that if a font size ever becomes 0
                        // or negative, through e.g. `\fs -10` or `\fs10\fs-20`, it gets reset to
                        // its default value.
                        // We can do this in the first case, where an absolute non-positive value
                        // is specified, but not in the second case.
                        if parsed <= 0.0 {
                            FontSize::Reset(FontSizeDelta::ZERO)
                        } else {
                            FontSize::Set(parsed)
                        }
                    }
                }
            }
            None => FontSize::Reset(FontSizeDelta::ZERO),
        }
    } else if twa.tag::<false>("bord") {
        local.border = maybe_both_dimensions(twa.float_arg(0));
    } else if twa.tag::<true>("move") {
        if global.position.is_none() && (twa.nargs() == 4 || twa.nargs() == 6) {
            let timing = (twa.nargs() == 6).then(|| {
                let t1 = twa.int_arg(4).unwrap();
                let t2 = twa.int_arg(5).unwrap();

                if t1 < t2 {
                    MoveTiming {
                        start_time: Milliseconds(t1),
                        end_time: Milliseconds(t2),
                    }
                } else {
                    MoveTiming {
                        start_time: Milliseconds(t2),
                        end_time: Milliseconds(t1),
                    }
                }
            });

            global.position = Some(PositionOrMove::Move(Move {
                initial_position: Position {
                    x: twa.float_arg(0).unwrap(),
                    y: twa.float_arg(1).unwrap(),
                },
                final_position: Position {
                    x: twa.float_arg(2).unwrap(),
                    y: twa.float_arg(3).unwrap(),
                },
                timing,
            }));
        }
    } else if twa.tag::<false>("frx") {
        local.text_rotation.x = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("fry") {
        local.text_rotation.y = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("frz") || twa.tag::<false>("fr") {
        local.text_rotation.z = resettable(twa.float_arg(0));
    } else if twa.tag::<false>("fn") {
        local.font_name = resettable(twa.string_arg(0).map(|name| lstrip(name).to_owned()));
    } else if twa.tag::<false>("alpha") {
        let resettable_transparency = resettable(twa.transparency_arg(0));
        local.primary_transparency = resettable_transparency;
        local.secondary_transparency = resettable_transparency;
        local.border_transparency = resettable_transparency;
        local.shadow_transparency = resettable_transparency;
    } else if twa.tag::<false>("an") {
        // Don't set the alignment more than once
        if global.alignment.is_keep() {
            use Resettable::*;
            global.alignment = match twa.int_arg(0) {
                Some(1) => Override(Alignment {
                    vertical: VerticalAlignment::Sub,
                    horizontal: HorizontalAlignment::Left,
                }),
                Some(2) => Override(Alignment {
                    vertical: VerticalAlignment::Sub,
                    horizontal: HorizontalAlignment::Center,
                }),
                Some(3) => Override(Alignment {
                    vertical: VerticalAlignment::Sub,
                    horizontal: HorizontalAlignment::Right,
                }),
                Some(4) => Override(Alignment {
                    vertical: VerticalAlignment::Center,
                    horizontal: HorizontalAlignment::Left,
                }),
                Some(5) => Override(Alignment {
                    vertical: VerticalAlignment::Center,
                    horizontal: HorizontalAlignment::Center,
                }),
                Some(6) => Override(Alignment {
                    vertical: VerticalAlignment::Center,
                    horizontal: HorizontalAlignment::Right,
                }),
                Some(7) => Override(Alignment {
                    vertical: VerticalAlignment::Top,
                    horizontal: HorizontalAlignment::Left,
                }),
                Some(8) => Override(Alignment {
                    vertical: VerticalAlignment::Top,
                    horizontal: HorizontalAlignment::Center,
                }),
                Some(9) => Override(Alignment {
                    vertical: VerticalAlignment::Top,
                    horizontal: HorizontalAlignment::Right,
                }),
                Some(_) | None => Reset,
            }
        }
    } else if twa.tag::<false>("a") {
        if global.alignment.is_keep() {
            use Resettable::*;
            global.alignment = match twa.int_arg(0) {
                Some(1) => Override(Alignment {
                    vertical: VerticalAlignment::Sub,
                    horizontal: HorizontalAlignment::Left,
                }),
                Some(2) => Override(Alignment {
                    vertical: VerticalAlignment::Sub,
                    horizontal: HorizontalAlignment::Center,
                }),
                Some(3) => Override(Alignment {
                    vertical: VerticalAlignment::Sub,
                    horizontal: HorizontalAlignment::Right,
                }),
                // “vsfilter quirk: handle illegal \a8 and \a4 like \a5”
                Some(5 | 4 | 8) => Override(Alignment {
                    vertical: VerticalAlignment::Top,
                    horizontal: HorizontalAlignment::Left,
                }),
                Some(6) => Override(Alignment {
                    vertical: VerticalAlignment::Top,
                    horizontal: HorizontalAlignment::Center,
                }),
                Some(7) => Override(Alignment {
                    vertical: VerticalAlignment::Top,
                    horizontal: HorizontalAlignment::Right,
                }),
                Some(9) => Override(Alignment {
                    vertical: VerticalAlignment::Center,
                    horizontal: HorizontalAlignment::Left,
                }),
                Some(10) => Override(Alignment {
                    vertical: VerticalAlignment::Center,
                    horizontal: HorizontalAlignment::Center,
                }),
                Some(11) => Override(Alignment {
                    vertical: VerticalAlignment::Center,
                    horizontal: HorizontalAlignment::Right,
                }),
                Some(_) | None => Reset,
            }
        }
    } else if twa.tag::<true>("pos") {
        if global.position.is_none() {
            global.position = twa.position_args().map(PositionOrMove::Position);
        }
    } else if twa.tag::<true>("fade") || twa.tag::<true>("fad") {
        if global.fade.is_none() {
            // libass does not differentiate the two fade types by name,
            // only by argument count.
            global.fade = match twa.nargs() {
                2 => {
                    // fad
                    Some(Fade::Simple(SimpleFade {
                        fade_in_duration: Milliseconds(twa.int_arg(0).unwrap()),
                        fade_out_duration: Milliseconds(twa.int_arg(1).unwrap()),
                    }))
                }
                7 => {
                    // fade
                    Some(Fade::Complex(ComplexFade {
                        transparency_before: twa.int_arg(0).unwrap(),
                        transparency_main: twa.int_arg(1).unwrap(),
                        transparency_after: twa.int_arg(2).unwrap(),
                        fade_in_start: Milliseconds(twa.int_arg(3).unwrap()),
                        fade_in_end: Milliseconds(twa.int_arg(4).unwrap()),
                        fade_out_start: Milliseconds(twa.int_arg(5).unwrap()),
                        fade_out_end: Milliseconds(twa.int_arg(6).unwrap()),
                    }))
                }
                _ => None,
            }
        }
    } else if twa.tag::<true>("org") {
        if global.origin.is_none() {
            global.origin = twa.position_args();
        }
    } else if twa.tag::<true>("t") {
        let (interval, acceleration) = match twa.nargs() {
            4 => (
                Some(AnimationInterval {
                    start: Milliseconds(twa.int_arg(0).unwrap()),
                    end: Milliseconds(twa.int_arg(1).unwrap()),
                }),
                twa.float_arg(2).unwrap(),
            ),
            3 => {
                // Although we do match *this* obscure edge case...
                // “VSFilter compatibility (because we can): parse the
                // timestamps differently depending on argument count”
                #[allow(clippy::cast_possible_truncation)]
                (
                    Some(AnimationInterval {
                        start: Milliseconds(twa.float_arg(0).unwrap() as i32),
                        end: Milliseconds(twa.float_arg(1).unwrap() as i32),
                    }),
                    1.0,
                )
            }
            2 => (None, twa.float_arg(0).unwrap()),
            1 => (None, 1.0),
            _ => return true,
        };

        if twa.has_backslash_arg {
            let mut inner_global = Global::empty();
            let animated_tags = twa.string_arg(twa.arguments.len() - 1).unwrap();
            let mut inner_block = parse_tag_block(animated_tags, &mut inner_global);

            let global_animatable = inner_global.split_animatable();
            if global_animatable != GlobalAnimatable::empty() {
                // It is in fact possible to have multiple global (clip)
                // animations, with different behaviour than if only one
                // of them were specified.
                global.animations.push(Animation {
                    modifiers: global_animatable,
                    acceleration,
                    interval,
                });
            }

            let local_animatable = inner_block.new_local.split_animatable();
            if local_animatable != LocalAnimatable::empty() {
                local.animations.push(Animation {
                    modifiers: local_animatable,
                    acceleration,
                    interval,
                });
            }

            // Merge leftover non-animatable global and local properties
            // into their respective targets.
            // This includes animations: nested animations behave like concatenated animations.
            // `\t(A,B,C,D\t(E,F,G,H))` is equivalent to `\t(A,B,C,D)\t(E,F,G,H)`.
            global.override_from(&inner_global);
            local.override_from(&inner_block.new_local, true);
        }
    } else if twa.tag::<true>("clip") {
        parse_clip(global, &twa, Clip::Contained, Clip::Contained);
    } else if twa.tag::<false>("c") || twa.tag::<false>("1c") {
        local.primary_colour = resettable(twa.colour_arg(0));
    } else if twa.tag::<false>("2c") {
        local.secondary_colour = resettable(twa.colour_arg(0));
    } else if twa.tag::<false>("3c") {
        local.border_colour = resettable(twa.colour_arg(0));
    } else if twa.tag::<false>("4c") {
        local.shadow_colour = resettable(twa.colour_arg(0));
    } else if twa.tag::<false>("1a") {
        local.primary_transparency = resettable(twa.transparency_arg(0));
    } else if twa.tag::<false>("2a") {
        local.secondary_transparency = resettable(twa.transparency_arg(0));
    } else if twa.tag::<false>("3a") {
        local.border_transparency = resettable(twa.transparency_arg(0));
    } else if twa.tag::<false>("4a") {
        local.shadow_transparency = resettable(twa.transparency_arg(0));
    } else if twa.tag::<false>("r") {
        // Clear previous overrides in this block, but keep the drawing baseline offset,
        // which is not reset by `\r`
        let prev_pbo = local.drawing_baseline_offset;
        *local = Local::empty();
        local.drawing_baseline_offset = prev_pbo;

        block.reset = if twa.nargs() > 0 {
            Some(Reset::ResetToStyle(twa.string_arg(0).unwrap().to_owned()))
        } else {
            Some(Reset::Reset)
        };
    } else if twa.tag::<false>("be") {
        local.soften = resettable(twa.int_arg(0));
    } else if twa.tag::<false>("b") {
        use Resettable::*;
        local.font_weight = match twa.int_arg(0) {
            Some(0) => Override(FontWeight::BoldToggle(false)),
            Some(1) => Override(FontWeight::BoldToggle(true)),
            Some(weight) if weight >= 100 => {
                Override(FontWeight::Numeric(weight.try_into().unwrap()))
            }
            Some(_) | None => Reset,
        }
    } else if twa.tag::<false>("i") {
        local.italic = resettable(twa.bool_arg(0));
    } else if twa.tag::<false>("kt") {
        local
            .karaoke
            .set_absolute(Centiseconds(twa.float_arg(0).unwrap_or(0.0)));
    } else if twa.tag::<false>("kf") || twa.tag::<false>("K") {
        local.karaoke.add_relative(
            KaraokeEffect::FillSweep,
            Centiseconds(twa.float_arg(0).unwrap_or(100.0)),
        );
    } else if twa.tag::<false>("ko") {
        local.karaoke.add_relative(
            KaraokeEffect::BorderInstant,
            Centiseconds(twa.float_arg(0).unwrap_or(100.0)),
        );
    } else if twa.tag::<false>("k") {
        local.karaoke.add_relative(
            KaraokeEffect::FillInstant,
            Centiseconds(twa.float_arg(0).unwrap_or(100.0)),
        );
    } else if twa.tag::<false>("shad") {
        // “VSFilter compatibility: clip for \shad but not for \[xy]shad”
        let maybe_val = resettable(twa.float_arg(0).map(|val| val.max(0.0)));
        local.shadow.x = maybe_val;
        local.shadow.y = maybe_val;
    } else if twa.tag::<false>("s") {
        local.strike_out = resettable(twa.bool_arg(0));
    } else if twa.tag::<false>("u") {
        local.underline = resettable(twa.bool_arg(0));
    } else if twa.tag::<false>("pbo") {
        local.drawing_baseline_offset = Some(twa.float_arg(0).unwrap_or(0.0));
    } else if twa.tag::<false>("p") {
        let scale = twa.int_arg(0).unwrap_or(0).max(0);
        if scale == 0 {
            block.end_previous_drawing = true;
        } else {
            block.new_drawing_scale = Some(scale);
        }
    } else if twa.tag::<false>("q") {
        global.wrap_style = match twa.int_arg(0) {
            Some(x) if (0..=3).contains(&x) => Resettable::Override(WrapStyle::from(x)),
            Some(_) | None => Resettable::Reset,
        };
    } else if twa.tag::<false>("fe") {
        local.font_encoding = resettable(twa.int_arg(0));
    } else {
        return false;
    }

    true
}

/// Convert a potentially present argument into an `Override` if it is present,
/// or into a `Reset` if it is not, matching the behaviour of most ASS tags.
fn resettable<T>(option: Option<T>) -> Resettable<T> {
    match option {
        Some(value) => Resettable::Override(value),
        None => Resettable::Reset,
    }
}

fn maybe_both_dimensions(option: Option<f64>) -> Maybe2D {
    match option {
        Some(value) => Maybe2D {
            x: Resettable::Override(value),
            y: Resettable::Override(value),
        },
        None => Maybe2D {
            x: Resettable::Reset,
            y: Resettable::Reset,
        },
    }
}

fn lstrip(str: &str) -> &str {
    &str[str
        .find(|char| char != ' ' && char != '\\')
        .unwrap_or(str.len())..]
}

fn parse_paren_args<'a>(paren_args: &'a str, twa: &mut TagWithArguments<'a>) {
    use ParenArgsParseState::*;

    if paren_args.is_empty() {
        return;
    }

    let mut state = Before;
    let mut arg_start_bytes = 0;
    let mut arg_end_bytes: Option<usize> = None;

    for (byte_index, next_char) in paren_args.char_indices() {
        state = match state {
            Before => match next_char {
                // Skip spaces, like above
                ' ' | '\t' => Before,
                ',' => {
                    twa.push_argument(&paren_args[arg_start_bytes..byte_index]);
                    arg_start_bytes = byte_index + 1; // Ignore the comma itself
                    Before
                }
                '\\' => {
                    twa.has_backslash_arg = true;

                    // Consume the rest of the argument,
                    // disregarding commas
                    BackslashArgument
                }
                ')' => {
                    arg_end_bytes = Some(byte_index);
                    break;
                }
                _ => {
                    arg_start_bytes = byte_index;
                    GenericArgument
                }
            },
            GenericArgument => match next_char {
                ',' => {
                    twa.push_argument(&paren_args[arg_start_bytes..byte_index]);
                    arg_start_bytes = byte_index + 1; // Ignore the comma itself
                    Before
                }
                '\\' => {
                    twa.has_backslash_arg = true;
                    BackslashArgument
                }
                ')' => {
                    arg_end_bytes = Some(byte_index);
                    break;
                }
                _ => GenericArgument,
            },
            BackslashArgument => match next_char {
                ')' => {
                    arg_end_bytes = Some(byte_index);
                    break;
                }
                _ => BackslashArgument,
            },
        }
    }

    let end = arg_end_bytes.unwrap_or(paren_args.len());

    // Don't include closing parenthesis
    twa.push_argument(&paren_args[arg_start_bytes..end]);
}

enum ParenArgsParseState {
    Before,
    GenericArgument,
    BackslashArgument,
}

struct TagWithArguments<'a> {
    first_part: &'a str,

    /// List of strings that might serve as arguments.
    /// These are not parsed and may in fact be formatted completely invalidly.
    arguments: Vec<&'a str>,
    has_backslash_arg: bool,
    tag_found: bool,
}

impl<'a> TagWithArguments<'a> {
    fn push_argument(&mut self, arg_str: &'a str) {
        if let Some(last_non_space) = arg_str.rfind(|char| char != ' ' && char != '\t') {
            self.arguments.push(&arg_str[0..=last_non_space]);
        }
    }

    fn nargs(&self) -> usize {
        self.arguments.len()
    }

    fn tag<const COMPLEX: bool>(&mut self, tag_name: &str) -> bool {
        assert!(
            !self.tag_found,
            "tried to call tag(), but the tag has already been found"
        );

        if self.first_part.starts_with(tag_name) {
            self.tag_found = true;
            if !COMPLEX {
                self.push_argument(&self.first_part[tag_name.bytes().len()..]);
            }
            true
        } else {
            false
        }
    }

    fn float_arg(&self, index: usize) -> Option<f64> {
        self.arguments.get(index).map(|arg_str| {
            assert!(!arg_str.is_empty());
            fast_float::parse_partial::<f64, _>(trim_start_ctype_isspace(arg_str))
                .ok()
                .map_or(0.0, |(value, _digits)| value)
        })
    }

    fn int_arg(&self, index: usize) -> Option<i32> {
        self.string_arg(index).map(|arg| parse_prefix_i32(arg, 10))
    }

    fn string_arg(&self, index: usize) -> Option<&'a str> {
        match self.arguments.get(index) {
            Some(slice) => {
                assert!(!slice.is_empty());
                Some(*slice)
            }
            None => None,
        }
    }

    /// Note that this function returns `Some(false)` for present but
    /// non-numeric arguments! This matches libass behaviour.
    fn bool_arg(&self, index: usize) -> Option<bool> {
        self.int_arg(index).and_then(|val: i32| match val {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        })
    }

    fn transparency_arg(&self, index: usize) -> Option<Transparency> {
        self.hex_arg(index).map(Transparency)
    }

    fn colour_arg(&self, index: usize) -> Option<Colour> {
        #[allow(clippy::cast_sign_loss)] // Yes, libass allows specifying negative colours.
        self.hex_arg(index)
            .map(|val: i32| Colour::from_bgr_packed(val as u32))
    }

    fn hex_arg(&self, index: usize) -> Option<i32> {
        self.string_arg(index).map(|arg| {
            arg.find(|char| char != '&' && char != 'H')
                .map_or(0, |first_value_char| {
                    parse_prefix_i32(&arg[first_value_char..], 16)
                })
        })
    }

    fn position_args(&self) -> Option<Position> {
        (self.nargs() == 2).then(|| Position {
            x: self.float_arg(0).unwrap(),
            y: self.float_arg(1).unwrap(),
        })
    }
}

/// Equivalent to libass' `mystrtoi32`.
/// Tries to parse as many numeric characters as possible
/// from the beginning of `str`, and returns 0 if parsing fails entirely.
/// Also handles i32 overflows gracefully by first parsing as i64.
fn parse_prefix_i32(str: &str, radix: u32) -> i32 {
    let str = trim_start_ctype_isspace(str);

    let (slice, sign) = match str.chars().next() {
        Some('+') => {
            (&str[1..], 1) // consume sign
        }
        Some('-') => (&str[1..], -1),
        Some(_) => (str, 1),
        None => return 0,
    };
    let num_end = slice
        .find(|char: char| !char.is_digit(radix))
        .unwrap_or(slice.len());
    let maybe_parsed = i64::from_str_radix(&slice[0..num_end], radix)
        .ok()
        .map(|num| num * sign);
    i64_to_i32_truncate(
        maybe_parsed
            .unwrap_or(0_i64)
            .clamp(i32::MIN.into(), i32::MAX.into()),
    )
}

#[allow(clippy::cast_possible_truncation)]
fn i64_to_i32_truncate(val: i64) -> i32 {
    val as i32
}

/// Trims whitespace from the start of a string, according to the non-Unicode-aware definition of
/// C's `isspace` function. This is required to match libass behaviour in the admittedly extremely
/// obscure case that someone specifies a numeric literal preceded by a Unicode whitespace.
fn trim_start_ctype_isspace(str: &str) -> &str {
    str.trim_start_matches(|char: char| {
        matches!(char, '\x20' | '\x0c' | '\x0a' | '\x0d' | '\x09' | '\x0b')
    })
}

fn parse_clip<R, V>(global: &mut Global, twa: &TagWithArguments, rect_clip: R, vector_clip: V)
where
    R: FnOnce(Rectangle) -> Clip<Rectangle>,
    V: FnOnce(Drawing) -> Clip<Drawing>,
{
    if twa.nargs() == 4 {
        let rect = Rectangle {
            x1: twa.int_arg(0).unwrap(),
            y1: twa.int_arg(1).unwrap(),
            x2: twa.int_arg(2).unwrap(),
            y2: twa.int_arg(3).unwrap(),
        };
        global.rectangle_clip = Some(rect_clip(rect));
    } else if global.vector_clip.is_none() {
        // Vector clips do not override their predecessors
        let scale: i32 = match twa.nargs() {
            2 => twa.int_arg(0).unwrap(),
            1 => 1,
            _ => return,
        };

        let commands = twa.string_arg(twa.nargs() - 1).unwrap();
        let drawing = Drawing {
            scale,
            commands: commands.to_owned(),
        };

        global.vector_clip = Some(vector_clip(drawing));
    }

    // As specifying a clip overrides all previous clips, it will also override clip animations.
    // There are no global animations other than clip ones, so clearing all global animations
    // suffices to clear previous clip animations
    global.animations.clear();
}

fn end_span(
    spans: &mut Vec<Span>,
    span_text: String,
    last_local: Box<Local>,
    drawing: Option<Drawing>,
    end_drawing: bool,
) -> Option<Drawing> {
    if let Some(mut drawing) = drawing {
        if end_drawing {
            drawing.commands = span_text;
            spans.push(Span::Drawing(*last_local, drawing));
            None
        } else {
            // If there is an override tag in the middle of a drawing,
            // the previous commands are discarded.
            drawing.commands.clear();

            // We still need to keep the local tags, since they may contain text override
            // tags that will apply to text following the drawing.
            spans.push(Span::Tags(*last_local, String::new()));

            Some(drawing)
        }
    } else {
        spans.push(Span::Tags(*last_local, span_text));
        None
    }
}

pub fn simplify(s0: Vec<Span>) -> Vec<Span> {
    use Span::*;

    // Remove empty texts and drawings
    let mut s1: Vec<Span> = vec![];
    for span in s0 {
        if !span.is_empty() {
            s1.push(span);
        }
    }

    // Try to merge spans into their predecessors
    let mut s2: Vec<Span> = vec![];
    for span in s1 {
        match span {
            Tags(local, text) => match s2.pop() {
                Some(prev_span) => match prev_span {
                    // Merge with preceding tags, if the preceding text is empty
                    Tags(mut prev_local, prev_text) if prev_text.is_empty() => {
                        prev_local.override_from(&local, true);
                        s2.push(Tags(prev_local, text));
                    }
                    // Merge with preceding drawing, if it has no commands
                    Drawing(mut prev_local, prev_drawing) if prev_drawing.is_empty() => {
                        prev_local.override_from(&local, true);
                        s2.push(Tags(prev_local, text));
                    }
                    _ => {
                        s2.push(prev_span);
                        s2.push(Tags(local, text));
                    }
                },
                None => s2.push(Tags(local, text)),
            },
            Drawing(local, drawing) => match s2.pop() {
                Some(prev_span) => match prev_span {
                    // Merge with preceding tags, if the preceding text is empty
                    Tags(mut prev_local, prev_text) if prev_text.is_empty() => {
                        prev_local.override_from(&local, true);
                        s2.push(Drawing(prev_local, drawing));
                    }
                    // Merge with preceding drawing, if it has no commands
                    Drawing(mut prev_local, prev_drawing) if prev_drawing.is_empty() => {
                        prev_local.override_from(&local, true);
                        s2.push(Drawing(prev_local, drawing));
                    }
                    _ => {
                        s2.push(prev_span);
                        s2.push(Drawing(local, drawing));
                    }
                },
                None => s2.push(Drawing(local, drawing)),
            },
            Reset => {
                if let Some(prev_span) = s2.last_mut() {
                    if prev_span.is_reset() {
                        *prev_span = Reset;
                    } else {
                        s2.push(Reset);
                    }
                } else {
                    // A reset at the beginning of the line does nothing,
                    // so we can skip it
                }
            }
            ResetToStyle(style_name) => match s2.last_mut() {
                // Overwrite preceding reset, if it exists
                Some(prev_span) => {
                    if prev_span.is_reset() {
                        *prev_span = ResetToStyle(style_name);
                    } else {
                        s2.push(ResetToStyle(style_name));
                    }
                }
                None => s2.push(ResetToStyle(style_name)),
            },
        }
    }

    // Remove spans without content from the end
    let mut last_non_empty_index = 0;
    for (i, span) in s2.iter().enumerate() {
        if !span.content_is_empty() {
            last_non_empty_index = i;
        }
    }
    s2.truncate(last_non_empty_index + 1);

    s2
}

struct TagBlock {
    reset: Option<Reset>,
    new_local: Local,
    new_drawing_scale: Option<i32>,
    end_previous_drawing: bool,
}

impl TagBlock {
    #[cfg(test)]
    fn empty() -> Self {
        Self {
            reset: None,
            new_local: Local::empty(),
            new_drawing_scale: None,
            end_previous_drawing: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Reset {
    Reset,
    ResetToStyle(String),
}

#[cfg(test)]
mod tests {
    use assert_matches2::assert_matches;

    use crate::nde::tags::{Karaoke, KaraokeOnset};

    use super::*;

    #[test]
    fn no_tags() {
        let text = "this text has no tags";
        let (global, spans) = raw(text);
        assert_eq!(*global, Global::empty());
        assert_eq!(spans.len(), 1);
        assert_matches!(&spans[0], Span::Tags(local, span_text));
        assert_eq!(local, &Local::empty());
        assert_eq!(span_text, text);
    }

    #[test]
    fn span_tags() {
        let (global, spans) = raw("before{\\i1}after");
        assert_eq!(*global, Global::empty());
        assert_eq!(spans.len(), 2);
        assert_matches!(&spans[0], Span::Tags(local, text));
        assert_eq!(*local, Local::empty());
        assert_eq!(text, "before");
        assert_matches!(&spans[1], Span::Tags(local, text));
        assert_eq!(
            *local,
            Local {
                italic: Resettable::Override(true),
                ..Default::default()
            }
        );
        assert_eq!(text, "after");

        let (global, spans) = raw("{\\pos(10,11)}text");
        assert_eq!(
            *global,
            Global {
                position: Some(PositionOrMove::Position(Position { x: 10.0, y: 11.0 })),
                ..Default::default()
            }
        );
        assert_eq!(spans.len(), 2);
        assert_matches!(&spans[0], Span::Tags(local, text));
        assert_eq!(*local, Local::empty());
        assert_eq!(text, "");
        assert_matches!(&spans[1], Span::Tags(local, text));
        assert_eq!(*local, Local::empty());
        assert_eq!(text, "text");
    }

    #[test]
    fn span_reset() {
        let (global, spans) = raw("a{\\rA\\r}b{\\rB}{\\r}c");
        assert_eq!(*global, Global::empty());
        assert_eq!(spans.len(), 7);
        assert_matches!(&spans[0], Span::Tags(local, text));
        assert_eq!(*local, Local::empty());
        assert_eq!(text, "a");
        assert_matches!(&spans[1], Span::Reset);
        assert_matches!(&spans[2], Span::Tags(local, text));
        assert_eq!(*local, Local::empty());
        assert_eq!(text, "b");
        assert_matches!(&spans[3], Span::ResetToStyle(style_name));
        assert_eq!(style_name, "B");
        assert_matches!(&spans[4], Span::Tags(local, text));
        assert_eq!(*local, Local::empty());
        assert_eq!(text, "");
        assert_matches!(&spans[5], Span::Reset);
        assert_matches!(&spans[6], Span::Tags(local, text));
        assert_eq!(*local, Local::empty());
        assert_eq!(text, "c");

        let (global, spans) = raw("a{\\fsp10\\r\\fax20}b");
        assert_eq!(*global, Global::empty());
        assert_eq!(spans.len(), 3);
        assert_matches!(&spans[0], Span::Tags(local, text));
        assert_eq!(*local, Local::empty());
        assert_eq!(text, "a");
        assert_matches!(&spans[1], Span::Reset);
        assert_matches!(&spans[2], Span::Tags(local, text));
        assert_eq!(local.letter_spacing, Resettable::Keep);
        assert_eq!(local.text_shear.x, Resettable::Override(20.0));
        assert_eq!(text, "b");
    }

    #[test]
    fn span_drawing() {
        let (global, spans) = raw("a{\\1c&HFF0000&\\p2}b{\\p0\\p1}c{\\p0}d");
        assert_eq!(*global, Global::empty());
        assert_eq!(spans.len(), 4);
        assert_matches!(&spans[0], Span::Tags(local, text));
        assert_eq!(*local, Local::empty());
        assert_eq!(text, "a");
        assert_matches!(&spans[1], Span::Drawing(local, Drawing { scale, commands }));
        assert_eq!(
            *local,
            Local {
                primary_colour: Resettable::Override(Colour {
                    red: 0,
                    green: 0,
                    blue: 0xff,
                }),
                ..Default::default()
            }
        );
        assert_eq!(*scale, 2);
        assert_eq!(commands, "b");
        assert_matches!(&spans[2], Span::Drawing(local, Drawing { scale, commands }));
        assert_eq!(*local, Local::empty());
        assert_eq!(*scale, 1);
        assert_eq!(commands, "c");
        assert_matches!(&spans[3], Span::Tags(local, text));
        assert_eq!(*local, Local::empty());
        assert_eq!(text, "d");
    }

    #[test]
    fn global_override() {
        use Resettable::*;

        let mut global = Global::empty();
        parse_tag_block(
            "\\an5\\an8\\clip(1,2,3,4)\\iclip(5,6,7,8)\\iclip(aaa)\\clip(bbb)\\pos(123,456)\\move(1,2,3,4)\\fad(1,2)\\fade(1,2,3,4,5,6,7)\\org(1,2)\\org(3,4)",
            &mut global,
        );

        // These tags should NOT override their predecessors.
        assert_eq!(
            global.alignment,
            Override(Alignment {
                vertical: VerticalAlignment::Center,
                horizontal: HorizontalAlignment::Center,
            })
        );
        assert_matches!(global.position, Some(PositionOrMove::Position(_)));
        assert_matches!(global.fade, Some(Fade::Simple(_)));
        assert_eq!(global.origin, Some(Position { x: 1.0, y: 2.0 }));
        assert_matches!(global.vector_clip, Some(Clip::Inverse(_)));

        // These tags SHOULD override their predecessors.
        assert_matches!(global.rectangle_clip, Some(Clip::Inverse(_)));
    }

    #[test]
    fn default_values() {
        use Resettable::*;

        let mut global = Global::empty();
        let block = parse_tag_block("\\xbord\\ybord\\xshad\\yshad\\fax\\fay\\iclip\\blur\\fscx\\fscy\\fsp\\fs\\frx\\fry\\frz\\fn\\an\\pos\\fade\\org\\t\\1c\\2c\\3c\\4c\\1a\\2a\\3a\\4a\\be\\b\\i\\kt\\s\\u\\pbo\\p\\q\\fe", &mut global);

        assert_matches!(block.new_local.border.x, Reset);
        assert_matches!(block.new_local.border.y, Reset);
        assert_matches!(block.new_local.shadow.x, Reset);
        assert_matches!(block.new_local.shadow.y, Reset);
        assert_matches!(block.new_local.text_shear.x, Reset);
        assert_matches!(block.new_local.text_shear.y, Reset);
        assert_matches!(global.rectangle_clip, None);
        assert_matches!(global.vector_clip, None);
        assert_matches!(block.new_local.gaussian_blur, Reset);
        assert_matches!(block.new_local.font_scale.x, Reset);
        assert_matches!(block.new_local.font_scale.y, Reset);
        assert_matches!(block.new_local.letter_spacing, Reset);
        assert_eq!(
            block.new_local.font_size,
            FontSize::Reset(FontSizeDelta::ZERO)
        );
        assert_matches!(block.new_local.text_rotation.x, Reset);
        assert_matches!(block.new_local.text_rotation.y, Reset);
        assert_matches!(block.new_local.text_rotation.z, Reset);
        assert_matches!(block.new_local.font_name, Reset);
        assert_matches!(global.alignment, Reset);
        assert_matches!(global.position, None);
        assert_matches!(global.fade, None);
        assert_matches!(global.origin, None);
        assert_eq!(block.new_local.animations.len(), 0);
        assert_matches!(block.new_local.primary_colour, Reset);
        assert_matches!(block.new_local.secondary_colour, Reset);
        assert_matches!(block.new_local.border_colour, Reset);
        assert_matches!(block.new_local.shadow_colour, Reset);
        assert_matches!(block.new_local.primary_transparency, Reset);
        assert_matches!(block.new_local.secondary_transparency, Reset);
        assert_matches!(block.new_local.border_transparency, Reset);
        assert_matches!(block.new_local.shadow_transparency, Reset);
        assert_matches!(block.new_local.soften, Reset);
        assert_matches!(block.new_local.font_weight, Reset);
        assert_matches!(block.new_local.italic, Reset);
        assert_eq!(
            block.new_local.karaoke,
            Karaoke {
                effect: None,
                onset: KaraokeOnset::Absolute(Centiseconds(0.0)),
            }
        );
        assert_matches!(block.new_local.strike_out, Reset);
        assert_matches!(block.new_local.underline, Reset);
        assert_eq!(block.new_local.drawing_baseline_offset, Some(0.0));
        assert!(block.end_previous_drawing);
        assert_matches!(global.wrap_style, Reset);
        assert_matches!(block.new_local.font_encoding, Reset);

        let mut global = Global::empty();
        let block = parse_tag_block(
            "\\bord\\move\\shad\\fsc\\alpha\\a\\fad\\clip\\kf",
            &mut global,
        );
        assert_matches!(block.new_local.border.x, Reset);
        assert_matches!(block.new_local.border.y, Reset);
        assert_matches!(global.position, None);
        assert_matches!(block.new_local.shadow.x, Reset);
        assert_matches!(block.new_local.shadow.y, Reset);
        assert_matches!(block.new_local.font_scale.x, Reset);
        assert_matches!(block.new_local.font_scale.y, Reset);
        assert_matches!(block.new_local.primary_transparency, Reset);
        assert_matches!(block.new_local.secondary_transparency, Reset);
        assert_matches!(block.new_local.border_transparency, Reset);
        assert_matches!(block.new_local.shadow_transparency, Reset);
        assert_matches!(global.alignment, Reset);
        assert_matches!(global.fade, None);
        assert_matches!(global.vector_clip, None);
        assert_matches!(global.rectangle_clip, None);
        assert_eq!(
            block.new_local.karaoke,
            Karaoke {
                effect: Some((KaraokeEffect::FillSweep, Centiseconds(100.0))),
                onset: KaraokeOnset::NoDelay,
            }
        );

        let mut global = Global::empty();
        let block = parse_tag_block("\\ko", &mut global);
        assert_eq!(
            block.new_local.karaoke,
            Karaoke {
                effect: Some((KaraokeEffect::BorderInstant, Centiseconds(100.0))),
                onset: KaraokeOnset::NoDelay,
            }
        );

        let mut global = Global::empty();
        let block = parse_tag_block("\\k", &mut global);
        assert_eq!(
            block.new_local.karaoke,
            Karaoke {
                effect: Some((KaraokeEffect::FillInstant, Centiseconds(100.0))),
                onset: KaraokeOnset::NoDelay,
            }
        );
    }

    #[test]
    fn override_values() {
        use Resettable::*;

        let mut global = Global::empty();
        let block = parse_tag_block("\\xbord1\\ybord2\\xshad3\\yshad4\\fax5\\fay6\\clip(70,80,90,100)\\iclip(7,8,9,10)\\iclip(1,abc)\\clip(2,def)\\blur11\\fscx12\\fscy13\\fsp14\\fs15\\frx16\\fry17\\frz18\\fnAlegreya\\an5\\pos(19,20)\\fade(0,255,0,0,1000,2000,3000)\\org(21,22)\\t(\\xbord23)\\1c&HFF0000&\\2c&H00FF00&\\3c&H0000FF&\\4c&HFF00FF&\\1a&H22&\\2a&H44&\\3a&H66&\\4a&H88&\\be24\\b1\\i1\\kt25\\s1\\u1\\pbo26\\p1\\q1\\fe1", &mut global);

        assert_eq!(block.new_local.border.x, Override(1.0));
        assert_eq!(block.new_local.border.y, Override(2.0));
        assert_eq!(block.new_local.shadow.x, Override(3.0));
        assert_eq!(block.new_local.shadow.y, Override(4.0));
        assert_eq!(block.new_local.text_shear.x, Override(5.0));
        assert_eq!(block.new_local.text_shear.y, Override(6.0));
        assert_eq!(
            global.rectangle_clip,
            Some(Clip::Inverse(Rectangle {
                x1: 7,
                y1: 8,
                x2: 9,
                y2: 10,
            }))
        );
        assert_eq!(
            global.vector_clip,
            Some(Clip::Inverse(Drawing {
                scale: 1,
                commands: "abc".to_owned(),
            }))
        );
        assert_eq!(block.new_local.gaussian_blur, Override(11.0));
        assert_eq!(block.new_local.font_scale.x, Override(12.0));
        assert_eq!(block.new_local.font_scale.y, Override(13.0));
        assert_eq!(block.new_local.letter_spacing, Override(14.0));
        assert_eq!(block.new_local.font_size, FontSize::Set(15.0));
        assert_eq!(block.new_local.text_rotation.x, Override(16.0));
        assert_eq!(block.new_local.text_rotation.y, Override(17.0));
        assert_eq!(block.new_local.text_rotation.z, Override(18.0));
        assert_eq!(block.new_local.font_name, Override("Alegreya".to_owned()));
        assert_eq!(
            global.alignment,
            Override(Alignment {
                horizontal: HorizontalAlignment::Center,
                vertical: VerticalAlignment::Center,
            })
        );
        assert_eq!(
            global.position,
            Some(PositionOrMove::Position(Position { x: 19.0, y: 20.0 }))
        );
        assert_eq!(
            global.fade,
            Some(Fade::Complex(ComplexFade {
                transparency_before: 0,
                transparency_main: 255,
                transparency_after: 0,
                fade_in_start: Milliseconds(0),
                fade_in_end: Milliseconds(1000),
                fade_out_start: Milliseconds(2000),
                fade_out_end: Milliseconds(3000),
            }))
        );
        assert_eq!(global.origin, Some(Position { x: 21.0, y: 22.0 }));
        assert_eq!(block.new_local.animations.len(), 1);
        assert_eq!(
            block.new_local.animations[0],
            Animation {
                modifiers: LocalAnimatable {
                    border: Maybe2D {
                        x: Override(23.0),
                        y: Keep,
                    },
                    ..Default::default()
                },
                acceleration: 1.0,
                interval: None,
            }
        );
        assert_eq!(
            block.new_local.primary_colour,
            Override(Colour {
                red: 0,
                green: 0,
                blue: 0xff,
            })
        );
        assert_eq!(
            block.new_local.secondary_colour,
            Override(Colour {
                red: 0,
                green: 0xff,
                blue: 0,
            })
        );
        assert_eq!(
            block.new_local.border_colour,
            Override(Colour {
                red: 0xff,
                green: 0,
                blue: 0,
            })
        );
        assert_eq!(
            block.new_local.shadow_colour,
            Override(Colour {
                red: 0xff,
                green: 0,
                blue: 0xff,
            })
        );
        assert_eq!(
            block.new_local.primary_transparency,
            Override(Transparency(0x22))
        );
        assert_eq!(
            block.new_local.secondary_transparency,
            Override(Transparency(0x44))
        );
        assert_eq!(
            block.new_local.border_transparency,
            Override(Transparency(0x66))
        );
        assert_eq!(
            block.new_local.shadow_transparency,
            Override(Transparency(0x88))
        );
        assert_eq!(block.new_local.soften, Override(24));
        assert_eq!(
            block.new_local.font_weight,
            Override(FontWeight::BoldToggle(true))
        );
        assert_eq!(block.new_local.italic, Override(true));
        assert_eq!(
            block.new_local.karaoke,
            Karaoke {
                effect: None,
                onset: KaraokeOnset::Absolute(Centiseconds(25.0)),
            }
        );
        assert_eq!(block.new_local.strike_out, Override(true));
        assert_eq!(block.new_local.underline, Override(true));
        assert_eq!(block.new_local.drawing_baseline_offset, Some(26.0));
        assert!(!block.end_previous_drawing);
        assert_eq!(block.new_drawing_scale, Some(1));
        assert_eq!(global.wrap_style, Override(WrapStyle::EndOfLine));
        assert_eq!(block.new_local.font_encoding, Override(1));

        let mut global = Global::empty();
        let block = parse_tag_block(
            "\\bord1\\move(2,3,4,5)\\shad6\\fsc7\\alpha&H08&\\a5\\fad(450,550)\\clip(2,m 0 0 s 100 0 100 100 0 100 c)\\kf8\\b500",
            &mut global,
        );
        assert_eq!(block.new_local.border.x, Override(1.0));
        assert_eq!(block.new_local.border.y, Override(1.0));
        assert_eq!(
            global.position,
            Some(PositionOrMove::Move(Move {
                initial_position: Position { x: 2.0, y: 3.0 },
                final_position: Position { x: 4.0, y: 5.0 },
                timing: None,
            }))
        );
        assert_eq!(block.new_local.shadow.x, Override(6.0));
        assert_eq!(block.new_local.shadow.y, Override(6.0));
        // `\fsc` can only reset, not override
        assert_eq!(block.new_local.font_scale.x, Reset);
        assert_eq!(block.new_local.font_scale.y, Reset);
        assert_eq!(
            block.new_local.primary_transparency,
            Override(Transparency(0x08))
        );
        assert_eq!(
            block.new_local.secondary_transparency,
            Override(Transparency(0x08))
        );
        assert_eq!(
            block.new_local.border_transparency,
            Override(Transparency(0x08))
        );
        assert_eq!(
            block.new_local.shadow_transparency,
            Override(Transparency(0x08))
        );
        assert_eq!(
            global.alignment,
            Override(Alignment {
                vertical: VerticalAlignment::Top,
                horizontal: HorizontalAlignment::Left,
            })
        );
        assert_eq!(
            global.fade,
            Some(Fade::Simple(SimpleFade {
                fade_in_duration: Milliseconds(450),
                fade_out_duration: Milliseconds(550),
            }))
        );
        assert_eq!(
            global.vector_clip,
            Some(Clip::Contained(Drawing {
                scale: 2,
                commands: "m 0 0 s 100 0 100 100 0 100 c".to_owned(),
            }))
        );
        assert_eq!(
            block.new_local.karaoke,
            Karaoke {
                effect: Some((KaraokeEffect::FillSweep, Centiseconds(8.0))),
                onset: KaraokeOnset::NoDelay,
            }
        );
        assert_eq!(
            block.new_local.font_weight,
            Override(FontWeight::Numeric(500))
        );

        let mut global = Global::empty();
        let block = parse_tag_block("\\ko9\\fs+10", &mut global);
        assert_eq!(
            block.new_local.karaoke,
            Karaoke {
                effect: Some((KaraokeEffect::BorderInstant, Centiseconds(9.0))),
                onset: KaraokeOnset::NoDelay,
            }
        );
        assert_eq!(
            block.new_local.font_size,
            FontSize::Delta(FontSizeDelta(10.0))
        );

        let mut global = Global::empty();
        let block = parse_tag_block("\\k10\\fs-11", &mut global);
        assert_eq!(
            block.new_local.karaoke,
            Karaoke {
                effect: Some((KaraokeEffect::FillInstant, Centiseconds(10.0))),
                onset: KaraokeOnset::NoDelay,
            }
        );
        assert_eq!(
            block.new_local.font_size,
            FontSize::Delta(FontSizeDelta(-11.0))
        );
    }

    #[test]
    fn resettable_helper() {
        use Resettable::*;

        assert_eq!(resettable(Some(123)), Override(123));
        assert_eq!(resettable::<i32>(None), Reset);
    }

    #[test]
    fn individual_tags() {
        use Resettable::*;

        let alpha_reset = test_single_local("alpha");
        assert_eq!(alpha_reset.primary_transparency, Reset);
        assert_eq!(alpha_reset.secondary_transparency, Reset);
        assert_eq!(alpha_reset.border_transparency, Reset);
        assert_eq!(alpha_reset.shadow_transparency, Reset);

        let alpha_override = test_single_local("alpha&H34&");
        assert_eq!(
            alpha_override.primary_transparency,
            Override(Transparency(0x34))
        );
        assert_eq!(
            alpha_override.secondary_transparency,
            Override(Transparency(0x34))
        );
        assert_eq!(
            alpha_override.border_transparency,
            Override(Transparency(0x34))
        );
        assert_eq!(
            alpha_override.shadow_transparency,
            Override(Transparency(0x34))
        );

        assert_eq!(test_single_global("an").alignment, Reset);
        assert_eq!(
            test_single_global("an5").alignment,
            Override(Alignment {
                vertical: VerticalAlignment::Center,
                horizontal: HorizontalAlignment::Center,
            })
        );
        assert_eq!(
            test_single_global("a10").alignment,
            Override(Alignment {
                vertical: VerticalAlignment::Center,
                horizontal: HorizontalAlignment::Center,
            })
        );

        assert_matches!(test_single_global("fad(1,2)").fade, Some(Fade::Simple(_)));
        assert_matches!(test_single_global("fade(1,2)").fade, Some(Fade::Simple(_)));
        assert_matches!(
            test_single_global("fad(1,2,3,4,5,6,7)").fade,
            Some(Fade::Complex(_))
        );
        assert_matches!(
            test_single_global("fade(1,2,3,4,5,6,7)").fade,
            Some(Fade::Complex(_))
        );

        let colour = test_single_local("1c&FFAA11");
        assert_eq!(
            colour.primary_colour,
            Override(Colour {
                red: 0x11,
                green: 0xaa,
                blue: 0xff,
            })
        );
    }

    #[test]
    fn animation() {
        let local = test_single_local("t(1,2,3,\\fsp10)");
        assert_eq!(local.animations.len(), 1);
        let anim = &local.animations[0];
        assert_eq!(
            anim.interval,
            Some(AnimationInterval {
                start: Milliseconds(1),
                end: Milliseconds(2),
            })
        );
        assert!((anim.acceleration - 3.0).abs() < f64::EPSILON);
        assert_eq!(anim.modifiers.letter_spacing, Resettable::Override(10.0));

        assert_matches!(
            &test_single_global("t(\\clip(1,2,3,4))").animations[0]
                .modifiers
                .clip,
            Some(Clip::Contained(_))
        );

        let mut global = Global::empty();
        parse_tag_block("\\t(\\clip(1,2,3,4))\\t(\\clip(5,6,7,8))", &mut global);
        assert_eq!(global.animations.len(), 2);
    }

    #[test]
    fn nested_animation() {
        let local = test_single_local("t(1,2,3,\\fsp10\\t(4,5,6\\xshad20)");
        assert_eq!(local.animations.len(), 2);

        assert_matches!(&local.animations[0], Animation { modifiers, .. });
        assert_eq!(modifiers.letter_spacing, Resettable::Override(10.0));

        assert_matches!(&local.animations[1], Animation { modifiers, .. });
        assert_eq!(modifiers.shadow.x, Resettable::Override(20.0));
    }

    #[test]
    fn animation_non_animatable() {
        let mut global = Global::empty();
        let block = parse_tag_block(
            "\\t(\\fsp10)\\t(\\fnAlegreya)\\t(\\clip(1,2,3,4))\\t(\\iclip(abc))",
            &mut global,
        );

        assert_eq!(global.animations.len(), 1);
        assert_eq!(block.new_local.animations.len(), 1);

        assert_matches!(global.vector_clip, Some(_));
        assert_matches!(block.new_local.font_name, Resettable::Override(_));
    }

    #[test]
    fn reset() {
        assert_eq!(test_tag("r").1.reset, Some(Reset::Reset));
        assert_eq!(
            test_tag("rStyle").1.reset,
            Some(Reset::ResetToStyle("Style".to_owned()))
        );
        assert_eq!(
            test_tag("rStyle)").1.reset,
            Some(Reset::ResetToStyle("Style)".to_owned()))
        );

        let mut global = Global::empty();

        assert_eq!(
            parse_tag_block("\\r(Style)", &mut global).reset,
            Some(Reset::ResetToStyle("Style".to_owned()))
        );
        assert_eq!(
            parse_tag_block("\\r(Style))", &mut global).reset,
            Some(Reset::ResetToStyle("Style".to_owned()))
        );
    }

    fn test_single_local(tag: &str) -> Local {
        test_tag(tag).1.new_local
    }

    fn test_single_global(tag: &str) -> Global {
        test_tag(tag).0
    }

    fn test_tag(tag: &str) -> (Global, TagBlock) {
        let mut global = Global::empty();
        let mut block = TagBlock::empty();

        assert!(
            parse_tag(tag, &mut global, &mut block),
            "should have parsed a tag in test_tag -- input: {tag}"
        );

        (global, block)
    }

    #[test]
    fn tag_with_arguments_simple() {
        let mut twa = TagWithArguments {
            first_part: "fax100",
            arguments: vec![],
            has_backslash_arg: false,
            tag_found: false,
        };

        assert!(!twa.tag::<false>("not_fax"));
        assert!(twa.tag::<false>("fax"));
        assert_eq!(twa.arguments.len(), 1);
        assert_eq!(twa.int_arg(0), Some(100));
    }

    #[test]
    fn argument_parse() {
        let twa = TagWithArguments {
            first_part: "",
            arguments: vec![
                "0",
                "aa",
                "+",
                "1234",
                "1234aa",
                "+1234aa",
                "-1234aa",
                "1234.56aa",
                "+1234.56aa",
                "-1234.56aa",
                "1234.56.78",
                "++123",
                "100000000000000",
                "-100000000000000",
                "&HFF&",
                "&HFFFFF&",
                "H",
                "&HFFAA11&",
                "1",
            ],
            has_backslash_arg: false,
            tag_found: true,
        };

        assert_eq!(twa.int_arg(0), Some(0));
        assert_eq!(twa.int_arg(1), Some(0));
        assert_eq!(twa.int_arg(2), Some(0));
        assert_eq!(twa.int_arg(3), Some(1234));
        assert_eq!(twa.int_arg(4), Some(1234));
        assert_eq!(twa.int_arg(5), Some(1234));
        assert_eq!(twa.int_arg(6), Some(-1234));
        assert_eq!(twa.int_arg(7), Some(1234));
        assert_eq!(twa.int_arg(8), Some(1234));
        assert_eq!(twa.int_arg(9), Some(-1234));
        assert_eq!(twa.int_arg(10), Some(1234));
        assert_eq!(twa.int_arg(11), Some(0));
        assert_eq!(twa.int_arg(12), Some(i32::MAX));
        assert_eq!(twa.int_arg(13), Some(i32::MIN));
        assert_eq!(twa.int_arg(twa.arguments.len()), None); // out of bounds

        assert_eq!(twa.float_arg(0), Some(0.0));
        assert_eq!(twa.float_arg(1), Some(0.0));
        assert_eq!(twa.float_arg(2), Some(0.0));
        assert_eq!(twa.float_arg(3), Some(1234.0));
        assert_eq!(twa.float_arg(4), Some(1234.0));
        assert_eq!(twa.float_arg(5), Some(1234.0));
        assert_eq!(twa.float_arg(6), Some(-1234.0));
        assert_eq!(twa.float_arg(7), Some(1234.56));
        assert_eq!(twa.float_arg(8), Some(1234.56));
        assert_eq!(twa.float_arg(9), Some(-1234.56));
        assert_eq!(twa.float_arg(10), Some(1234.56));
        assert_eq!(twa.float_arg(11), Some(0.0));
        assert_eq!(twa.float_arg(12), Some(100_000_000_000_000.0));
        assert_eq!(twa.float_arg(13), Some(-100_000_000_000_000.0));
        assert_eq!(twa.float_arg(twa.arguments.len()), None);

        assert_eq!(twa.transparency_arg(0), Some(Transparency(0)));
        assert_eq!(twa.transparency_arg(1), Some(Transparency(0xaa)));
        assert_eq!(twa.transparency_arg(2), Some(Transparency(0)));
        assert_eq!(twa.transparency_arg(3), Some(Transparency(0x1234)));
        assert_eq!(twa.transparency_arg(4), Some(Transparency(0x123_4aa)));
        assert_eq!(twa.transparency_arg(14), Some(Transparency(0xff)));
        assert_eq!(twa.transparency_arg(15), Some(Transparency(0xfffff)));
        assert_eq!(twa.transparency_arg(16), Some(Transparency(0)));
        assert_eq!(twa.transparency_arg(17), Some(Transparency(0xffa_a11)));

        assert_eq!(twa.colour_arg(0), Some(Colour::BLACK));
        assert_eq!(
            twa.colour_arg(1),
            Some(Colour {
                red: 0xaa,
                green: 0,
                blue: 0,
            })
        );
        assert_eq!(twa.colour_arg(2), Some(Colour::BLACK));
        assert_eq!(
            twa.colour_arg(3),
            Some(Colour {
                red: 0x34,
                green: 0x12,
                blue: 0,
            })
        );
        assert_eq!(
            twa.colour_arg(4),
            Some(Colour {
                red: 0xaa,
                green: 0x34,
                blue: 0x12,
            })
        );
        assert_eq!(
            twa.colour_arg(14),
            Some(Colour {
                red: 0xff,
                green: 0,
                blue: 0,
            })
        );
        assert_eq!(
            twa.colour_arg(15),
            Some(Colour {
                red: 0xff,
                green: 0xff,
                blue: 0x0f,
            })
        );
        assert_eq!(twa.colour_arg(16), Some(Colour::BLACK));
        assert_eq!(
            twa.colour_arg(17),
            Some(Colour {
                red: 0x11,
                green: 0xaa,
                blue: 0xff,
            })
        );

        assert_eq!(twa.bool_arg(0), Some(false));
        assert_eq!(twa.bool_arg(1), Some(false)); // `aa` gets parsed as 0 numerically
        assert_eq!(twa.bool_arg(3), None);
        assert_eq!(twa.bool_arg(18), Some(true));
    }

    #[test]
    fn argument_parse_position() {
        let twa = TagWithArguments {
            first_part: "",
            arguments: vec!["123", "456"],
            has_backslash_arg: false,
            tag_found: true,
        };

        assert_eq!(twa.position_args(), Some(Position { x: 123.0, y: 456.0 }));

        let twa2 = TagWithArguments {
            first_part: "",
            arguments: vec!["123"],
            has_backslash_arg: false,
            tag_found: true,
        };

        assert_eq!(twa2.position_args(), None);
    }

    #[test]
    fn position_empty_second_arg() {
        let mut global = Global::empty();
        let mut block = TagBlock::empty();
        parse_tag("pos(50,)", &mut global, &mut block);
        assert_eq!(global.position, None);
    }

    #[test]
    fn font_size_space_before_plus() {
        let mut global = Global::empty();
        let mut block = TagBlock::empty();
        parse_tag("fs +10", &mut global, &mut block);
        assert_eq!(block.new_local.font_size, FontSize::Set(10.0));
    }

    #[test]
    fn reset_after_pbo() {
        let mut global = Global::empty();
        let block = parse_tag_block("\\pbo50\\r", &mut global);
        assert_eq!(block.new_local.drawing_baseline_offset, Some(50.0));
    }

    #[test]
    fn simplification() {
        use Span::*;

        let non_empty = Local {
            italic: Resettable::Override(true),
            ..Default::default()
        };

        let empty_drawing = super::Drawing {
            scale: 1,
            commands: String::new(),
        };

        let non_empty_drawing = super::Drawing {
            scale: 1,
            commands: "Drawing".to_owned(),
        };

        let spans: Vec<Span> = vec![
            Reset,
            Tags(non_empty.clone(), "a".to_owned()),
            Tags(Local::empty(), String::new()),
            Tags(non_empty.clone(), "b".to_owned()),
            Drawing(non_empty.clone(), non_empty_drawing.clone()),
            Drawing(Local::empty(), empty_drawing.clone()),
            Drawing(non_empty.clone(), non_empty_drawing.clone()),
            Reset,
            ResetToStyle("A".to_owned()),
            Tags(non_empty.clone(), "c".to_owned()),
            ResetToStyle("B".to_owned()),
            Reset,
            Tags(non_empty.clone(), "d".to_owned()),
            Tags(non_empty.clone(), String::new()),
            Tags(non_empty.clone(), "e".to_owned()),
            Drawing(non_empty.clone(), empty_drawing.clone()),
            Tags(non_empty.clone(), "f".to_owned()),
            Tags(non_empty.clone(), String::new()),
            Drawing(non_empty.clone(), non_empty_drawing.clone()),
            Drawing(non_empty.clone(), empty_drawing.clone()),
            Drawing(non_empty.clone(), non_empty_drawing),
            Tags(non_empty.clone(), "g".to_owned()),
            ResetToStyle("C".to_owned()),
            Reset,
            Tags(non_empty.clone(), String::new()),
            Drawing(non_empty, empty_drawing),
        ];

        let simplified = simplify(spans);

        assert_eq!(simplified.len(), 13);
        assert_matches!(&simplified[0], Tags(_, _));
        assert_matches!(&simplified[1], Tags(_, _));
        assert_matches!(&simplified[2], Drawing(_, _));
        assert_matches!(&simplified[3], Drawing(_, _));
        assert_matches!(&simplified[4], ResetToStyle(_));
        assert_matches!(&simplified[5], Tags(_, _));
        assert_matches!(&simplified[6], Reset);
        assert_matches!(&simplified[7], Tags(_, _));
        assert_matches!(&simplified[8], Tags(_, _));
        assert_matches!(&simplified[9], Tags(_, _));
        assert_matches!(&simplified[10], Drawing(_, _));
        assert_matches!(&simplified[11], Drawing(_, _));
        assert_matches!(&simplified[12], Tags(_, _));
    }

    #[test]
    fn utility() {
        assert_eq!(lstrip("  abc "), "abc ");
        assert_eq!(lstrip("abc"), "abc");
        assert_eq!(lstrip(""), "");
    }
}
