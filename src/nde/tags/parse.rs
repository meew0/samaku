use crate::nde::tags::{
    Clip, ClipDrawing, ClipRectangle, FontSize, Maybe2D, Milliseconds, Move, MoveTiming, Position,
    PositionOrMove, Resettable,
};
use crate::nde::Span;
use crate::subtitle::{Alignment, HorizontalAlignment, VerticalAlignment};

use super::{Drawing, Global, Local, Transparency};

pub fn parse(text: &str) -> (Box<Global>, Vec<Span>) {
    let mut spans: Vec<Span> = vec![];
    let mut last_local = Box::new(Local::empty());
    let mut global = Box::new(Global::empty());

    let mut drawing: Option<Drawing> = None;

    let mut slice = text;

    let mut span_text = String::new();
    let mut maybe_block_start = false;

    'outer: while !slice.is_empty() {
        if maybe_block_start {
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

                // The tags might have contained a reset, which we have to deal with
                // before appending any following override tags in `end_span`.
                match reset {
                    Some(Reset::Reset) => spans.push(Span::Reset),
                    Some(Reset::ResetToStyle(style_name)) => {
                        spans.push(Span::ResetToStyle(style_name))
                    }
                    None => {}
                }

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

                // Check whether we need to set a new scale, or create a new drawing
                if let Some(new_drawing_scale) = new_drawing_scale {
                    match &mut drawing {
                        Some(existing_drawing) => {
                            existing_drawing.scale = new_drawing_scale

                            // We would also reset the commands here,
                            // but it has already been done in `end_span`, if necessary.
                        }
                        None => {
                            // Create a new drawing
                            drawing = Some(Drawing {
                                scale: new_drawing_scale,
                                commands: String::new(),
                            })
                        }
                    }
                }
            } else {
                span_text.push('{');
            }
        } else {
            let mut last_byte_index = 0;
            let mut escape = false;
            let mut char_iter = slice.char_indices();

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
    span_text = String::new();

    (global, spans)
}

fn parse_tag_block(block: &str, global: &mut Global) -> TagBlock {
    let mut tag_block = TagBlock {
        reset: None,
        new_local: Local::empty(),
        new_drawing_scale: None,
        end_previous_drawing: false,
    };

    use TagBlockParseState::*;
    let mut state = Comment;
    let mut tag_start_bytes = 0;

    for (byte_index, next_char) in block.char_indices() {
        state = match state {
            Comment => match next_char {
                '\\' => TagStart,
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
                ')' => {
                    parse_tag(
                        &block[tag_start_bytes..(byte_index + 1)],
                        global,
                        &mut tag_block,
                    );
                    Comment
                }
                _ => Parenthesis,
            },
        }
    }

    parse_tag(&block[tag_start_bytes..], global, &mut tag_block);

    tag_block
}

enum TagBlockParseState {
    Comment,
    TagStart,
    Tag,
    Parenthesis,
}

fn parse_tag(tag: &str, global: &mut Global, block: &mut TagBlock) -> bool {
    if tag.is_empty() {
        return false;
    }

    let paren_pos = tag.find('(');

    // Contains the name and potentially the first argument
    let first_part = &tag[0..paren_pos.unwrap_or(tag.bytes().len())];

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
        parse_clip(global, &twa, Clip::InverseRectangle, Clip::InverseVector);
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
                    '+' => Resettable::Override(FontSize::Increase(parsed)),
                    '-' => Resettable::Override(FontSize::Decrease(-parsed)),
                    _ => {
                        // libass has the additional behaviour that if a font size ever becomes 0
                        // or negative, through e.g. `\fs -10` or `\fs10\fs-20`, it gets reset to
                        // its default value.
                        // We can do this in the first case, where an absolute non-positive value
                        // is specified, but not in the second case.
                        if parsed <= 0.0 {
                            Resettable::Reset
                        } else {
                            Resettable::Override(FontSize::Set(parsed))
                        }
                    }
                }
            }
            None => Resettable::Reset,
        }
    } else if twa.tag::<false>("bord") {
        local.border = maybe_both_dimensions(twa.float_arg(0));
    } else if twa.tag::<true>("move") {
        if global.position.is_none() && (twa.nargs() == 4 || twa.nargs() == 6) {
            let timing = if twa.nargs() == 6 {
                let t1 = twa.int_arg(4).unwrap();
                let t2 = twa.int_arg(5).unwrap();

                Some(if t1 < t2 {
                    MoveTiming {
                        start_time: Milliseconds(t1),
                        end_time: Milliseconds(t2),
                    }
                } else {
                    MoveTiming {
                        start_time: Milliseconds(t2),
                        end_time: Milliseconds(t1),
                    }
                })
            } else {
                None
            };

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
        local.font_name = resettable(twa.string_arg(0).map(|name| lstrip(name).to_string()));
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
                Some(5) | Some(4) | Some(8) => Override(Alignment {
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
        todo!()
    } else if twa.tag::<true>("fade") || twa.tag::<true>("fad") {
        todo!()
    } else if twa.tag::<true>("org") {
        todo!()
    } else if twa.tag::<true>("t") {
        todo!()
    } else if twa.tag::<true>("clip") {
        parse_clip(global, &twa, Clip::Rectangle, Clip::Vector);
    } else if twa.tag::<false>("c") || twa.tag::<false>("1c") {
        todo!()
    } else if twa.tag::<false>("2c") {
        todo!()
    } else if twa.tag::<false>("3c") {
        todo!()
    } else if twa.tag::<false>("4c") {
        todo!()
    } else if twa.tag::<false>("1a") {
        todo!()
    } else if twa.tag::<false>("2a") {
        todo!()
    } else if twa.tag::<false>("3a") {
        todo!()
    } else if twa.tag::<false>("4a") {
        todo!()
    } else if twa.tag::<false>("r") {
        todo!()
    } else if twa.tag::<false>("be") {
        todo!()
    } else if twa.tag::<false>("b") {
        todo!()
    } else if twa.tag::<false>("i") {
        todo!()
    } else if twa.tag::<false>("kt") {
        todo!()
    } else if twa.tag::<false>("kf") || twa.tag::<false>("K") {
        todo!()
    } else if twa.tag::<false>("ko") {
        todo!()
    } else if twa.tag::<false>("k") {
        todo!()
    } else if twa.tag::<false>("shad") {
        todo!()
    } else if twa.tag::<false>("s") {
        todo!()
    } else if twa.tag::<false>("u") {
        todo!()
    } else if twa.tag::<false>("pbo") {
        todo!()
    } else if twa.tag::<false>("p") {
        todo!()
    } else if twa.tag::<false>("q") {
        todo!()
    } else if twa.tag::<false>("fe") {
        todo!()
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
    if paren_args.is_empty() {
        return;
    }

    use ParenArgsParseState::*;
    let mut state = BeforeArgument;
    let mut arg_start_bytes = 0;

    for (byte_index, next_char) in paren_args.char_indices() {
        state = match state {
            BeforeArgument => match next_char {
                // Skip spaces, like above
                ' ' | '\t' => BeforeArgument,
                '\\' => {
                    twa.has_backslash_arg = true;
                    arg_start_bytes = byte_index;
                    Argument
                }
                _ => {
                    arg_start_bytes = byte_index;
                    Argument
                }
            },
            Argument => match next_char {
                ',' => {
                    twa.push_argument(&paren_args[arg_start_bytes..byte_index]);
                    BeforeArgument
                }
                _ => Argument,
            },
        }
    }

    twa.push_argument(&paren_args[arg_start_bytes..]);
}

enum ParenArgsParseState {
    BeforeArgument,
    Argument,
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
            self.arguments.push(&arg_str[0..(last_non_space + 1)]);
        }
    }

    fn nargs(&self) -> usize {
        self.arguments.len()
    }

    fn tag<const COMPLEX: bool>(&mut self, tag_name: &str) -> bool {
        if self.tag_found {
            panic!("tried to call tag(), but the tag has already been found");
        }

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
            fast_float::parse_partial::<f64, _>(arg_str)
                .ok()
                .map(|(value, _digits)| value)
                .unwrap_or(0.0) // default value if parsing fails
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

    fn transparency_arg(&self, index: usize) -> Option<Transparency> {
        self.string_arg(index).map(|arg| {
            arg.find(|char| char != '&' && char != 'H')
                .map(|first_value_char| {
                    Transparency(parse_prefix_i32(&arg[first_value_char..], 16) as u8)
                })
                .unwrap_or(Transparency(0))
        })
    }
}

/// Equivalent to libass' `mystrtoi32`.
/// Tries to parse as many numeric characters as possible
/// from the beginning of `str`, and returns 0 if parsing fails entirely.
/// Also handles i32 overflows gracefully by first parsing as i64.
fn parse_prefix_i32(str: &str, radix: u32) -> i32 {
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
    maybe_parsed
        .unwrap_or(0i64)
        .clamp(i32::MIN.into(), i32::MAX.into()) as i32
}

fn parse_clip<R, V>(global: &mut Global, twa: &TagWithArguments, rect_clip: R, vector_clip: V)
where
    R: FnOnce(ClipRectangle) -> Clip,
    V: FnOnce(ClipDrawing) -> Clip,
{
    if twa.nargs() == 4 {
        let rect = ClipRectangle {
            x1: twa.int_arg(0).unwrap(),
            x2: twa.int_arg(1).unwrap(),
            y1: twa.int_arg(2).unwrap(),
            y2: twa.int_arg(3).unwrap(),
        };
        global.clip = Some(rect_clip(rect));
    } else {
        let scale: i32 = match twa.nargs() {
            2 => twa.int_arg(0).unwrap(),
            1 => 1,
            _ => return,
        };

        let commands = twa.string_arg(twa.nargs() - 1).unwrap();
        let drawing = ClipDrawing {
            scale,
            commands: commands.to_string(),
        };

        global.clip = Some(vector_clip(drawing));
    }
}

fn end_span(
    spans: &mut Vec<Span>,
    span_text: String,
    last_local: Box<Local>,
    drawing: Option<Drawing>,
    end_drawing: bool,
) -> Option<Drawing> {
    match drawing {
        Some(mut drawing) => {
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
        }
        None => {
            spans.push(Span::Tags(*last_local, span_text));
            None
        }
    }
}

fn simplify(spans: Vec<Span>) -> Vec<Span> {
    // TODO
    spans
}

struct TagBlock {
    reset: Option<Reset>,
    new_local: Local,
    new_drawing_scale: Option<f64>,
    end_previous_drawing: bool,
}

impl TagBlock {
    fn empty() -> Self {
        Self {
            reset: None,
            new_local: Local::empty(),
            new_drawing_scale: None,
            end_previous_drawing: false,
        }
    }
}

enum Reset {
    Reset,
    ResetToStyle(String),
}

struct State {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_tags() {
        let text = "this text has no tags";
        let (global, spans) = parse(text);
        assert_eq!(*global, Global::empty());
        assert_eq!(spans.len(), 1);
        match &spans[0] {
            Span::Tags(local, span_text) => {
                assert_eq!(span_text, text);
                assert_eq!(local, &Local::empty());
            }
            _ => panic!("found non-Tags span: {:?}", spans[0]),
        }
    }

    #[test]
    fn global_override() {
        use Resettable::*;

        let mut global = Global::empty();
        parse_tag_block("\\an5\\an8\\clip(1,2,3,4)\\iclip(aaa)", &mut global);

        // These tags should NOT override their predecessors.
        assert_eq!(
            global.alignment,
            Override(Alignment {
                vertical: VerticalAlignment::Center,
                horizontal: HorizontalAlignment::Center
            })
        );

        // These tags SHOULD override their predecessors.
        assert!(matches!(global.clip, Some(Clip::InverseVector(_))));
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
                horizontal: HorizontalAlignment::Center
            })
        );
        assert_eq!(
            test_single_global("a10").alignment,
            Override(Alignment {
                vertical: VerticalAlignment::Center,
                horizontal: HorizontalAlignment::Center
            })
        );
    }

    fn test_single_local(tag: &str) -> Local {
        let mut global = Global::empty();
        let mut block = TagBlock::empty();

        if !parse_tag(tag, &mut global, &mut block) {
            panic!(
                "should have parsed a tag in test_single_local -- input: {}",
                tag
            );
        }

        block.new_local
    }

    fn test_single_global(tag: &str) -> Global {
        let mut global = Global::empty();
        let mut block = TagBlock::empty();

        if !parse_tag(tag, &mut global, &mut block) {
            panic!(
                "should have parsed a tag in test_single_global -- input: {}",
                tag
            );
        }

        global
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
        assert_eq!(twa.float_arg(12), Some(100000000000000.0));
        assert_eq!(twa.float_arg(13), Some(-100000000000000.0));
        assert_eq!(twa.float_arg(twa.arguments.len()), None);

        assert_eq!(twa.transparency_arg(0), Some(Transparency(0)));
        assert_eq!(twa.transparency_arg(1), Some(Transparency(0xaa)));
        assert_eq!(twa.transparency_arg(2), Some(Transparency(0)));
        assert_eq!(twa.transparency_arg(3), Some(Transparency(0x34)));
        assert_eq!(twa.transparency_arg(4), Some(Transparency(0xaa)));
        assert_eq!(twa.transparency_arg(14), Some(Transparency(0xff)));
        assert_eq!(twa.transparency_arg(15), Some(Transparency(0xff)));
        assert_eq!(twa.transparency_arg(16), Some(Transparency(0)));
    }

    #[test]
    fn utility() {
        assert_eq!(lstrip("  abc "), "abc ");
        assert_eq!(lstrip("abc"), "abc");
        assert_eq!(lstrip(""), "");
    }
}
