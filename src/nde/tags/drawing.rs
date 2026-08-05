use super::emit;
use glam::DVec2;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawing {
    pub scale: i32,
    pub commands: Vec<Command>,
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

    #[must_use]
    pub fn parse(scale: i32, command_string: &str) -> Self {
        Self {
            scale,
            commands: Self::parse_commands(command_string),
        }
    }

    /// Drawing command parsing matching what libass does.
    ///
    /// Invalid commands are ignored. It does *not* match the rounding behavior,
    /// to ensure precision in drawing editing.
    #[must_use]
    pub fn parse_commands(command_string: &str) -> Vec<Command> {
        // libass reads drawings as NUL-terminated C strings, so nothing past a
        // NUL byte is part of the drawing.
        let (text, _) = command_string
            .split_once('\0')
            .unwrap_or((command_string, ""));

        Tokenizer::new(text).tokenize().unwrap_or_default()
    }

    #[must_use]
    pub fn emit_vector_clip(&self) -> impl emit::Value + '_ {
        struct EmitVectorClip<'a>(&'a Drawing);

        impl emit::Value for EmitVectorClip<'_> {
            fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
            where
                W: std::fmt::Write,
            {
                write!(sink, "{},", self.0.scale)?;
                emit_commands(sink, &self.0.commands)
            }
        }

        EmitVectorClip(self)
    }

    #[must_use]
    pub fn emit_inline(&self) -> impl emit::Value + '_ {
        struct EmitInline<'a>(&'a Drawing);

        impl emit::Value for EmitInline<'_> {
            fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
            where
                W: std::fmt::Write,
            {
                write!(sink, "{{\\p{}}}", self.0.scale)?;
                emit_commands(sink, &self.0.commands)?;
                write!(sink, "{{\\p0}}")?;
                Ok(())
            }
        }

        EmitInline(self)
    }
}

/// Equivalent to the state kept by libass' `drawing_tokenize`.
struct Tokenizer<'a> {
    /// The part of the drawing string that has not been consumed yet.
    rest: &'a str,

    commands: Vec<Command>,

    /// Number of points parsed so far.
    num_points: usize,

    /// Whether a root node has been created. Corresponds to libass' `root`
    /// being non-null.
    rooted: bool,

    /// Whether an `m` command was seen, even one that failed to parse.
    /// Corresponds to libass' `m_seen`.
    m_seen: bool,

    /// Whether there is a B-spline that a `c` command could close.
    spline_open: bool,
}

impl<'a> Tokenizer<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            rest: text,
            commands: vec![],
            num_points: 0,
            rooted: false,
            m_seen: false,
            spline_open: false,
        }
    }

    /// Returns `None` if the drawing should be rejected in its entirety, like
    /// libass' `drawing_tokenize` returning a null token list.
    fn tokenize(mut self) -> Option<Vec<Command>> {
        while let Some(command_char) = self.next_char() {
            // VSFilter compat (matching libass)
            //
            // > In guliverkli(2) VSFilter all drawings whose first known (but
            // > potentially invalid) command isn't m are rejected. xy-VSF and
            // > MPC-HC ISR later relaxed this (possibly inadvertenly), such that
            // > all known commands but n are ignored if there was no prior node
            // > yet. If an invalid m preceded n, the latter becomes the root
            // > node, otherwise if n comes before any other not-ignored command
            // > the entire drawing is rejected. 'p' is further restricted and
            // > ignored unless there are already >= 3 nodes.
            match command_char {
                'm' => {
                    self.m_seen = true;
                    if !self.rooted {
                        let Some(point) = self.next_point() else {
                            continue;
                        };
                        self.add_root(Command::MoveTo(point));
                    }
                    self.num_points += self.add_points(Command::MoveTo);
                }
                'n' => {
                    if !self.rooted {
                        let Some(point) = self.next_point() else {
                            continue;
                        };
                        if !self.m_seen {
                            return None;
                        }
                        self.add_root(Command::MoveNoClose(point));
                    }
                    self.num_points += self.add_points(Command::MoveNoClose);
                }
                'l' => {
                    if self.rooted {
                        self.num_points += self.add_points(Command::LineTo);
                    }
                }
                'b' => {
                    if self.rooted {
                        self.num_points += self.add_cubic_béziers();
                    }
                }
                's' => {
                    if !self.rooted {
                        continue;
                    }

                    // Only the initial three points make up the B-spline.
                    // All following ones extend it.
                    self.spline_open = true;
                    if !self.add_b_spline() {
                        self.spline_open = false;
                        continue;
                    }
                    self.num_points += 3;
                    self.extend_spline();
                }
                'p' => self.extend_spline(),
                'c' if self.spline_open => {
                    self.commands.push(Command::Close);
                    self.spline_open = false;
                }
                _ => {
                    // Ignore invalid commands
                }
            }
        }

        Some(self.commands)
    }

    /// Consumes and returns the next character, which is interpreted as a
    /// command.
    fn next_char(&mut self) -> Option<char> {
        let mut chars = self.rest.chars();
        let next = chars.next()?;
        self.rest = chars.as_str();
        Some(next)
    }

    /// Consumes the next point. Mirrors libass' `get_point`: if only the first
    /// of the two coordinates matches, it is still consumed.
    fn next_point(&mut self) -> Option<DVec2> {
        let (maybe_x, after_x) = parse_number(self.rest);
        self.rest = after_x;
        let x = maybe_x?;

        let (maybe_y, after_y) = parse_number(self.rest);
        self.rest = after_y;
        let y = maybe_y?;

        Some(DVec2::new(x, y))
    }

    fn add_root(&mut self, command: Command) {
        self.commands.push(command);
        self.num_points = 1;
        self.rooted = true;
    }

    /// Consumes points for as long as they match, turning each one into a
    /// command. Mirrors libass' `add_many_points` with a batch size of 1.
    /// Returns the number of points added.
    fn add_points(&mut self, make_command: fn(DVec2) -> Command) -> usize {
        let mut count = 0;

        while !self.rest.is_empty() {
            let Some(point) = self.next_point() else {
                break;
            };
            self.commands.push(make_command(point));
            count += 1;
        }

        count
    }

    /// Consumes points for as long as they match, turning every three of them
    /// into a cubic Bézier command. Mirrors libass' `add_many_points` with a
    /// batch size of 3: a trailing incomplete batch is consumed, but discarded.
    /// Returns the number of points added.
    fn add_cubic_béziers(&mut self) -> usize {
        let mut buffer = [DVec2::ZERO; 3];
        let mut count_total = 0;
        let mut count_batch = 0;

        while !self.rest.is_empty() {
            let Some(point) = self.next_point() else {
                break;
            };
            buffer[count_batch] = point;
            count_total += 1;
            count_batch += 1;

            if count_batch == buffer.len() {
                self.commands
                    .push(Command::CubicBézier(buffer[0], buffer[1], buffer[2]));
                count_batch = 0;
            }
        }

        count_total - count_batch
    }

    fn add_b_spline(&mut self) -> bool {
        let mut buffer = [DVec2::ZERO; 3];

        for slot in &mut buffer {
            let Some(point) = self.next_point() else {
                return false;
            };
            *slot = point;
        }

        self.commands
            .push(Command::BSpline(buffer[0], buffer[1], buffer[2]));
        true
    }

    // Used for `p` or for `s` with more than 3 points.
    fn extend_spline(&mut self) {
        if self.num_points >= 3 {
            self.num_points += self.add_points(Command::ExtendSpline);
        }
    }
}

/// Wrapper around `fast_float2` more closely matching libass' `mystrtod`.
///
/// Returns the parsed value, if it exists, together with the remainder of the string.
/// Note that the remainder does not include the number's characters even if no
/// value is returned.
fn parse_number(input: &str) -> (Option<f64>, &str) {
    // `ass_isspace`
    let after_space = input.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);

    let after_sign = after_space.strip_prefix(['-', '+']).unwrap_or(after_space);

    // The mantissa consists of digits and at most one decimal point.
    let mut mantissa_len = after_sign.len();
    let mut num_digits: usize = 0;
    let mut point_seen = false;
    for (index, char) in after_sign.char_indices() {
        if char.is_ascii_digit() {
            num_digits += 1;
        } else if char == '.' && !point_seen {
            point_seen = true;
        } else {
            mantissa_len = index;
            break;
        }
    }

    if num_digits == 0 {
        return (None, input);
    }

    let (_, after_mantissa) = after_sign.split_at(mantissa_len);

    // Do not backtrack if an `e` is not followed by any digits,
    // instead, just use an exponent of zero.
    let remainder = match after_mantissa.strip_prefix(['e', 'E']) {
        Some(after_e) => {
            let after_exponent_sign = after_e.strip_prefix(['-', '+']).unwrap_or(after_e);
            after_exponent_sign.trim_start_matches(|char: char| char.is_ascii_digit())
        }
        None => after_mantissa,
    };

    // Use `fast_float2` for the actual parsing
    let value = fast_float2::parse_partial::<f64, _>(after_space)
        .ok()
        .map_or(0.0, |(value, _digits)| value);

    (Some(value), remainder)
}

/// This implementation is only valid for vector clips
/// (added so that the `Clip` type bounds can be satisfied).
impl emit::Value for Drawing {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        self.emit_vector_clip().emit_value(sink)
    }
}

fn emit_commands<W: std::fmt::Write>(
    sink: &mut W,
    commands: &[Command],
) -> Result<(), std::fmt::Error> {
    use emit::Value as _;

    // libass rejects a drawing that starts with `n` unless an `m` — even an
    // invalid one — preceded it, see `Drawing::parse_commands`. Emitting a bare
    // `m` reproduces exactly that situation: it sets libass' `m_seen` flag,
    // fails to parse a point, and leaves the `n` to become the root node.
    if matches!(commands.first(), Some(&Command::MoveNoClose(_))) {
        write!(sink, "m ")?;
    }

    if let Some((last, head)) = commands.split_last() {
        for command in head {
            command.emit_value(sink)?;
            write!(sink, " ")?;
        }
        last.emit_value(sink)?;
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Move the cursor to the specified point.
    /// If this is not the first command in a drawing, it will close the previous path.
    /// Corresponds to `m`.
    MoveTo(DVec2),

    /// Move the cursor to the specified point, without closing the previous path.
    /// Corresponds to `n`.
    MoveNoClose(DVec2),

    /// Draws a straight line from the current cursor position to the given position,
    /// and moves the cursor there afterwards.
    /// Corresponds to `l`.
    LineTo(DVec2),

    /// Draws a cubic Bézier curve with the given three points, then moves the
    /// cursor to the last point.
    /// Corresponds to `b`.
    CubicBézier(DVec2, DVec2, DVec2),

    /// Draws a cubic B-spline with the given three points, which can later be
    /// extended using the `ExtendSpline` command.
    /// Corresponds to `s`.
    BSpline(DVec2, DVec2, DVec2),

    /// Adds another point to the current B-spline.
    /// Corresponds to `p`.
    ExtendSpline(DVec2),

    /// Closes the current B-spline, by repeating its first three points.
    /// Note that libass ignores this command if there is no B-spline to close;
    /// it does *not* close paths in general.
    /// Corresponds to `c`.
    Close,
}

impl emit::Value for Command {
    fn emit_value<W>(&self, sink: &mut W) -> Result<(), std::fmt::Error>
    where
        W: std::fmt::Write,
    {
        match *self {
            Command::MoveTo(pos) => {
                write!(sink, "m {} {}", pos.x, pos.y)
            }
            Command::MoveNoClose(pos) => {
                write!(sink, "n {} {}", pos.x, pos.y)
            }
            Command::LineTo(pos) => {
                write!(sink, "l {} {}", pos.x, pos.y)
            }
            Command::CubicBézier(p1, p2, p3) => {
                write!(
                    sink,
                    "b {} {} {} {} {} {}",
                    p1.x, p1.y, p2.x, p2.y, p3.x, p3.y
                )
            }
            Command::BSpline(p1, p2, p3) => {
                write!(
                    sink,
                    "s {} {} {} {} {} {}",
                    p1.x, p1.y, p2.x, p2.y, p3.x, p3.y
                )
            }
            Command::ExtendSpline(pos) => {
                write!(sink, "p {} {}", pos.x, pos.y)
            }
            Command::Close => {
                write!(sink, "c")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::dvec2;

    use super::{Command, Drawing};

    fn parse(command_string: &str) -> Vec<Command> {
        Drawing::parse_commands(command_string)
    }

    fn emit(commands: &[Command]) -> String {
        let mut string = String::new();
        super::emit_commands(&mut string, commands).unwrap();
        string
    }

    #[test]
    fn basic() {
        let commands = "m 0 0 l 100 0 100 100 0 100";
        let result = vec![
            Command::MoveTo(dvec2(0.0, 0.0)),
            Command::LineTo(dvec2(100.0, 0.0)),
            Command::LineTo(dvec2(100.0, 100.0)),
            Command::LineTo(dvec2(0.0, 100.0)),
        ];

        assert_eq!(parse(commands), result);

        let drawing = Drawing::parse(2, commands);
        assert_eq!(drawing.scale, 2);
        assert_eq!(drawing.commands, result);
        assert!(!drawing.is_empty());

        assert!(Drawing::parse(1, "invalid").is_empty());
    }

    #[test]
    fn edge_cases() {
        // Empty commands
        assert_eq!(parse(""), vec![]);
        assert_eq!(parse("   "), vec![]);
        assert_eq!(parse("hello"), vec![]);

        // Separators other than whitespace are not recognized
        assert_eq!(parse("m1 2"), vec![Command::MoveTo(dvec2(1.0, 2.0))]);
        assert_eq!(parse("m\n1\t2"), vec![Command::MoveTo(dvec2(1.0, 2.0))]);
        assert_eq!(parse("m 1,2"), vec![]);

        // A null byte terminates the drawing
        assert_eq!(
            parse("m 1 1\0 l 2 2"),
            vec![Command::MoveTo(dvec2(1.0, 1.0))]
        );

        // Everything but `n` is ignored while there is no node yet.
        assert_eq!(
            parse("l 1 1 b 1 1 2 2 3 3 s 1 1 2 2 3 3 p 1 1 c m 5 5 l 6 6"),
            vec![
                Command::MoveTo(dvec2(5.0, 5.0)),
                Command::LineTo(dvec2(6.0, 6.0)),
            ]
        );
    }

    #[test]
    fn move_commands() {
        // An `n` before any `m` rejects the entire drawing, but only if it is
        // followed by a valid point,
        // and an invalid `m` is sufficient to make it the root node instead.
        assert_eq!(parse("n 1 1 m 2 2"), vec![]);
        assert_eq!(parse("n m 1 1"), vec![Command::MoveTo(dvec2(1.0, 1.0))]);
        assert_eq!(
            parse("m x n 1 1"),
            vec![Command::MoveNoClose(dvec2(1.0, 1.0))]
        );
        assert_eq!(
            parse("m 1 1 n 2 2"),
            vec![
                Command::MoveTo(dvec2(1.0, 1.0)),
                Command::MoveNoClose(dvec2(2.0, 2.0)),
            ]
        );

        // `m`, `n`, `l` and `p` consume as many points as they can.
        assert_eq!(
            parse("m 1 2 3 4"),
            vec![
                Command::MoveTo(dvec2(1.0, 2.0)),
                Command::MoveTo(dvec2(3.0, 4.0)),
            ]
        );

        // A point whose second coordinate is missing is dropped, but its first
        // coordinate is still consumed.
        assert_eq!(
            parse("m 1 2 l 3 4 5"),
            vec![
                Command::MoveTo(dvec2(1.0, 2.0)),
                Command::LineTo(dvec2(3.0, 4.0)),
            ]
        );

        // Without the trailing `5` being consumed, it would be interpreted as
        // the start of a new command's coordinates
        assert_eq!(
            parse("m 1 2 l 3 4 5 6"),
            vec![
                Command::MoveTo(dvec2(1.0, 2.0)),
                Command::LineTo(dvec2(3.0, 4.0)),
                Command::LineTo(dvec2(5.0, 6.0)),
            ]
        );
    }

    #[test]
    fn cubic_bézier() {
        // After 3 points are consumed, the remaining ones are ignored
        assert_eq!(
            parse("m 0 0 b 1 1 2 2 3 3 4 4 5 5"),
            vec![
                Command::MoveTo(dvec2(0.0, 0.0)),
                Command::CubicBézier(dvec2(1.0, 1.0), dvec2(2.0, 2.0), dvec2(3.0, 3.0)),
            ]
        );
    }

    #[test]
    fn b_spline() {
        assert_eq!(
            parse("m 0 0 s 100 0 100 100 0 100 c"),
            vec![
                Command::MoveTo(dvec2(0.0, 0.0)),
                Command::BSpline(dvec2(100.0, 0.0), dvec2(100.0, 100.0), dvec2(0.0, 100.0)),
                Command::Close,
            ]
        );

        // After the initial three points, `s` keeps consuming points as if a `p`
        // followed it.
        assert_eq!(
            parse("m 0 0 s 1 1 2 2 3 3 4 4"),
            vec![
                Command::MoveTo(dvec2(0.0, 0.0)),
                Command::BSpline(dvec2(1.0, 1.0), dvec2(2.0, 2.0), dvec2(3.0, 3.0)),
                Command::ExtendSpline(dvec2(4.0, 4.0)),
            ]
        );

        // An `s` with fewer than three points gets ignored,
        // and also invalidates a previously started B spline.
        assert_eq!(
            parse("m 0 0 s 1 1 2 2 c"),
            vec![Command::MoveTo(dvec2(0.0, 0.0))]
        );

        assert_eq!(
            parse("m 0 0 s 1 1 2 2 3 3 s 4 4 c"),
            vec![
                Command::MoveTo(dvec2(0.0, 0.0)),
                Command::BSpline(dvec2(1.0, 1.0), dvec2(2.0, 2.0), dvec2(3.0, 3.0)),
            ]
        );
    }

    /// `c` is only valid directly after a b-spline, and only once.
    #[test]
    fn close() {
        assert_eq!(
            parse("m 0 0 l 1 1 c"),
            vec![
                Command::MoveTo(dvec2(0.0, 0.0)),
                Command::LineTo(dvec2(1.0, 1.0)),
            ]
        );

        assert_eq!(
            parse("m 0 0 s 1 1 2 2 3 3 c c"),
            vec![
                Command::MoveTo(dvec2(0.0, 0.0)),
                Command::BSpline(dvec2(1.0, 1.0), dvec2(2.0, 2.0), dvec2(3.0, 3.0)),
                Command::Close,
            ]
        );
    }

    /// `p` is ignored unless there are at least three points already, and
    /// unlike other commands it does not need a b-spline to extend.
    #[test]
    fn extend_spline() {
        assert_eq!(parse("m 0 0 p 1 1"), vec![Command::MoveTo(dvec2(0.0, 0.0))]);

        assert_eq!(
            parse("m 0 0 l 1 1 l 2 2 p 3 3"),
            vec![
                Command::MoveTo(dvec2(0.0, 0.0)),
                Command::LineTo(dvec2(1.0, 1.0)),
                Command::LineTo(dvec2(2.0, 2.0)),
                Command::ExtendSpline(dvec2(3.0, 3.0)),
            ]
        );

        // Closing a b-spline does not count towards the three points
        assert_eq!(
            parse("m 0 0 s 1 1 2 2 3 3 c p 4 4"),
            vec![
                Command::MoveTo(dvec2(0.0, 0.0)),
                Command::BSpline(dvec2(1.0, 1.0), dvec2(2.0, 2.0), dvec2(3.0, 3.0)),
                Command::Close,
                Command::ExtendSpline(dvec2(4.0, 4.0)),
            ]
        );
    }

    #[test]
    fn number_formats() {
        assert_eq!(
            parse("m -1.5 +2. l .25 00003"),
            vec![
                Command::MoveTo(dvec2(-1.5, 2.0)),
                Command::LineTo(dvec2(0.25, 3.0)),
            ]
        );

        assert_eq!(
            parse("m 1e2 1.5E-1"),
            vec![Command::MoveTo(dvec2(100.0, 0.15))]
        );

        // `ass_strtod` consumes an `e` and its sign even if no digits follow,
        // treating the exponent as zero
        assert_eq!(parse("m 1 1e"), vec![Command::MoveTo(dvec2(1.0, 1.0))]);
        assert_eq!(
            parse("m 1 1e- l 2 2"),
            vec![
                Command::MoveTo(dvec2(1.0, 1.0)),
                Command::LineTo(dvec2(2.0, 2.0)),
            ]
        );

        // A lone decimal point is not a number, so the point fails to parse and
        // the following coordinates are skipped as unknown commands
        assert_eq!(parse("m . 1 2"), vec![]);
        assert_eq!(parse("m . m 1 2"), vec![Command::MoveTo(dvec2(1.0, 2.0))]);
    }

    #[test]
    fn emit_round_trip() {
        let original = "m 0 0 n 1 1 l 2 2 b 3 3 4 4 5 5 s 6 6 7 7 8 8 p 9 9 c";
        let commands = parse(original);
        assert_eq!(emit(&commands), original);
        assert_eq!(parse(&emit(&commands)), commands);
    }
}
