use crate::nde::tags::parse::ParenArgsParseState::BeforeArgument;
use crate::nde::Span;

use super::{Drawing, Global, Local};

pub fn parse(text: &str) -> (Box<Global>, Vec<Span>) {
    let mut spans: Vec<Span> = vec![];
    let mut last_local = Box::new(Local::empty());
    let mut global = Box::new(Global::empty());

    let mut drawing: Option<Drawing> = Some(Drawing::empty());

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

fn parse_tag(tag: &str, global: &mut Global, block: &mut TagBlock) {
    if tag.is_empty() {
        return;
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

    if twa.tag::<false>("xbord") {
    } else if twa.tag::<false>("ybord") {
        todo!()
    } else if twa.tag::<false>("xshad") {
        todo!()
    } else if twa.tag::<false>("yshad") {
        todo!()
    } else if twa.tag::<false>("fax") {
        todo!()
    } else if twa.tag::<false>("fay") {
        todo!()
    } else if twa.tag::<true>("iclip") {
        todo!()
    } else if twa.tag::<false>("blur") {
        todo!()
    } else if twa.tag::<false>("fscx") {
        todo!()
    } else if twa.tag::<false>("fscy") {
        todo!()
    } else if twa.tag::<false>("fsc") {
        todo!()
    } else if twa.tag::<false>("fsp") {
        todo!()
    } else if twa.tag::<false>("fs") {
        todo!()
    } else if twa.tag::<false>("bord") {
        todo!()
    } else if twa.tag::<true>("move") {
        todo!()
    } else if twa.tag::<false>("frx") {
        todo!()
    } else if twa.tag::<false>("fry") {
        todo!()
    } else if twa.tag::<false>("frz") || twa.tag::<false>("fr") {
        todo!()
    } else if twa.tag::<false>("fn") {
        todo!()
    } else if twa.tag::<false>("alpha") {
        todo!()
    } else if twa.tag::<false>("an") {
        todo!()
    } else if twa.tag::<false>("a") {
        todo!()
    } else if twa.tag::<true>("pos") {
        todo!()
    } else if twa.tag::<true>("fade") || twa.tag::<true>("fad") {
        todo!()
    } else if twa.tag::<true>("org") {
        todo!()
    } else if twa.tag::<true>("t") {
        todo!()
    } else if twa.tag::<true>("clip") {
        todo!()
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
    }
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
                    twa.arguments.push(&paren_args[arg_start_bytes..byte_index]);
                    BeforeArgument
                }
                _ => Argument,
            },
        }
    }

    twa.arguments.push(&paren_args[arg_start_bytes..]);
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
    fn tag<const COMPLEX: bool>(&mut self, tag_name: &str) -> bool {
        if self.tag_found {
            panic!("tried to call tag(), but the tag has already been found");
        }

        if self.first_part.starts_with(tag_name) {
            self.tag_found = true;
            if !COMPLEX {
                self.arguments
                    .push(&self.first_part[tag_name.bytes().len()..]);
            }
            true
        } else {
            false
        }
    }

    fn float_arg(&self, index: usize) -> Option<f64> {
        self.arguments.get(index).and_then(|arg_str| {
            fast_float::parse_partial::<f64, _>(arg_str)
                .ok()
                .map(|(value, _digits)| value)
        })
    }

    fn int_arg(&self, index: usize) -> Option<i32> {
        let mut slice = match self.arguments.get(index) {
            Some(slice) => *slice,
            None => return None,
        };
        let sign = match slice.chars().next() {
            Some('+') => {
                slice = &slice[1..]; // consume sign
                1
            }
            Some('-') => {
                slice = &slice[1..];
                -1
            }
            Some(_) => 1,
            None => return None,
        };
        let num_end = slice
            .find(|char: char| !char.is_numeric())
            .unwrap_or(slice.len());
        slice[0..num_end].parse::<i32>().ok().map(|num| num * sign)
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

enum Reset {
    Reset,
    ResetToStyle(String),
}

struct State {}

#[cfg(test)]
mod tests {
    use super::*;

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
                "",
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
            ],
            has_backslash_arg: false,
            tag_found: true,
        };

        assert_eq!(twa.int_arg(0), None);
        assert_eq!(twa.int_arg(1), None);
        assert_eq!(twa.int_arg(2), None);
        assert_eq!(twa.int_arg(3), Some(1234));
        assert_eq!(twa.int_arg(4), Some(1234));
        assert_eq!(twa.int_arg(5), Some(1234));
        assert_eq!(twa.int_arg(6), Some(-1234));
        assert_eq!(twa.int_arg(7), Some(1234));
        assert_eq!(twa.int_arg(8), Some(1234));
        assert_eq!(twa.int_arg(9), Some(-1234));
        assert_eq!(twa.int_arg(10), Some(1234));
        assert_eq!(twa.int_arg(11), None);
        assert_eq!(twa.int_arg(12), None); // out of bounds

        assert_eq!(twa.float_arg(0), None);
        assert_eq!(twa.float_arg(1), None);
        assert_eq!(twa.float_arg(2), None);
        assert_eq!(twa.float_arg(3), Some(1234.0));
        assert_eq!(twa.float_arg(4), Some(1234.0));
        assert_eq!(twa.float_arg(5), Some(1234.0));
        assert_eq!(twa.float_arg(6), Some(-1234.0));
        assert_eq!(twa.float_arg(7), Some(1234.56));
        assert_eq!(twa.float_arg(8), Some(1234.56));
        assert_eq!(twa.float_arg(9), Some(-1234.56));
        assert_eq!(twa.float_arg(10), Some(1234.56));
        assert_eq!(twa.float_arg(11), None);
        assert_eq!(twa.float_arg(12), None);
    }
}
