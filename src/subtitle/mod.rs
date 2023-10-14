//! This module contains types for Samaku's internal representation
//! of subtitles, as well as the logic for compiling them to ASS
//! ones.

use crate::{media, nde};

pub mod ass;
pub mod compile;

/// An `Sline` (“samaku line”/“subtitle line”/“sign or line”/etc.),
/// in samaku terms, is one conceptual individual “subtitle”,
/// that is, a dialogue line, a complex sign, etc.
/// It may compile to multiple underlying ASS [`Event`]s.
#[derive(Debug, Clone)]
pub struct Sline {
    /// The time in milliseconds when this line first appears.
    pub start: StartTime,

    /// The time in milliseconds for which this line is shown
    /// beginning at the `start` time.
    pub duration: Duration,

    /// The layer index on which this line is shown. Elements on
    /// layers with higher numbers are shown above those on layers
    /// with lower numbers.
    pub layer_index: i32,

    /// The index of the style used for the line. If no style with
    /// this index exists, the default style (index 0) is used
    /// instead, which always exists.
    pub style_index: i32,

    /// If this line is not manually positioned using `\pos` tags,
    /// these margins determine its offset from the frame border.
    pub margins: Margins,

    /// The text shown for this line, potentially including ASS
    /// formatting tags.
    pub text: String,

    pub nde_filter_index: Option<usize>,
}

/// The time at which an element starts to be shown, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartTime(pub i64);

/// The duration for which an element is shown, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Duration(pub i64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Angle(pub f64);

/// 1.0 represents 100%
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub x: f64,
    pub y: f64,
}

/// Element- or style-specific left, right, and vertical margins
/// in pixels, corresponding to ASS `MarginL` etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Margins {
    pub left: i32,
    pub right: i32,
    pub vertical: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,

    /// How transparent this colour is. 255 means fully transparent.
    /// Corresponds to what libass confusingly calls “alpha”. To avoid this confusion,
    /// the term “alpha” will never be used in samaku.
    pub transparency: u8,
}

impl Colour {
    /// Converts a libass 32-bit packed colour into a `Colour`.
    #[must_use]
    pub fn unpack(packed: u32) -> Self {
        #[allow(clippy::unreadable_literal)]
        Self {
            red: ((packed & 0xff000000) >> 24) as u8,
            green: ((packed & 0x00ff0000) >> 16) as u8,
            blue: ((packed & 0x0000ff00) >> 8) as u8,
            transparency: (packed & 0x000000ff) as u8,
        }
    }

    /// Converts a colour into the 32 bit packed value used in libass.
    #[must_use]
    pub fn pack(&self) -> u32 {
        u32::from(self.red) << 24
            | u32::from(self.green) << 16
            | u32::from(self.blue) << 8
            | u32::from(self.transparency)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alignment {
    pub vertical: VerticalAlignment,
    pub horizontal: HorizontalAlignment,
}

impl Alignment {
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

    // Convert to a number to be used in the `\an` formatting tag.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyMode {
    Auto = 0,
    Left = 1,
    Center = 2,
    Right = 3,
}

impl From<i32> for JustifyMode {
    fn from(value: i32) -> Self {
        match value {
            x if x == Self::Left as i32 => Self::Left,
            x if x == Self::Center as i32 => Self::Center,
            x if x == Self::Right as i32 => Self::Right,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    /// Normal border style, with outline and shadow.
    Default = 1,

    /// What happens when you click “Opaque Box” in Aegisub.
    OpaqueBox = 3,

    /// Something libass-specific, seems mostly the same as OpaqueBox.
    Background = 4,
}

impl From<i32> for BorderStyle {
    fn from(value: i32) -> Self {
        match value {
            x if x == Self::OpaqueBox as i32 => Self::OpaqueBox,
            x if x == Self::Background as i32 => Self::Background,

            // It seems like all other int values are treated as equivalent to Default in libass,
            // so this conversion seems ok
            _ => Self::Default,
        }
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

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Style {
    pub name: String,
    pub font_name: String,

    pub font_size: f64,

    pub primary_colour: Colour,
    pub secondary_colour: Colour,
    pub outline_colour: Colour,
    pub back_colour: Colour,

    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike_out: bool,

    pub scale: Scale,
    pub spacing: f64,
    pub angle: Angle,

    pub border_style: BorderStyle,
    pub outline: f64,
    pub shadow: f64,

    pub alignment: Alignment,
    pub margins: Margins,

    /// “Windows font charset number”
    /// `-1` = autodetect, Aegisub's default seems to be `1`
    pub encoding: i32,

    pub blur: f64,
    pub justify: JustifyMode,
}

/// Ordered collection of [`Sline`]s with associated data.
/// For now, it's just a wrapper around [`Vec`].
/// Might become more advanced in the future.
pub struct SlineTrack {
    pub slines: Vec<Sline>,
    pub styles: Vec<Style>,
    pub filters: Vec<nde::Filter>,
    pub playback_resolution: Resolution,
}

impl SlineTrack {
    /// Returns true if and only if there are no slines in this track
    /// (there may still be some styles)
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slines.is_empty()
    }

    pub fn active_sline(&self, active_sline_index: Option<usize>) -> Option<&Sline> {
        match active_sline_index {
            Some(active_sline_index) => Some(&self.slines[active_sline_index]),
            None => None,
        }
    }

    pub fn active_sline_mut(&mut self, active_sline_index: Option<usize>) -> Option<&mut Sline> {
        match active_sline_index {
            Some(active_sline_index) => Some(&mut self.slines[active_sline_index]),
            None => None,
        }
    }

    pub fn active_nde_filter(&self, active_sline_index: Option<usize>) -> Option<&nde::Filter> {
        match self.active_sline(active_sline_index) {
            Some(active_sline) => match active_sline.nde_filter_index {
                Some(nde_filter_index) => Some(&self.filters[nde_filter_index]),
                None => None,
            },
            None => None,
        }
    }

    pub fn active_nde_filter_mut(
        &mut self,
        active_sline_index: Option<usize>,
    ) -> Option<&mut nde::Filter> {
        match self.active_sline(active_sline_index) {
            Some(active_sline) => match active_sline.nde_filter_index {
                Some(nde_filter_index) => Some(&mut self.filters[nde_filter_index]),
                None => None,
            },
            None => None,
        }
    }

    /// Compile subtitles in the given frame range to ASS.
    #[must_use]
    pub fn compile(
        &self,
        _frame_start: i32,
        _frame_count: i32,
        _frame_rate: media::FrameRate,
    ) -> Vec<self::ass::Event> {
        let mut counter = 0;
        let mut compiled: Vec<self::ass::Event> = vec![];

        for sline in &self.slines {
            match sline.nde_filter_index {
                Some(filter_index) => {
                    let graph = &self.filters[filter_index].graph;
                    match compile::nde(sline, graph, &mut counter) {
                        Ok(mut nde_result) => match &mut nde_result.events {
                            Some(events) => compiled.append(events),
                            None => println!("No output from NDE filter"),
                        },
                        Err(error) => {
                            println!("Got NdeError while running NDE filter: {error:?}");
                        }
                    }
                }
                None => compiled.push(compile::trivial(sline, &mut counter)),
            }
        }

        compiled
    }
}

impl Default for SlineTrack {
    fn default() -> Self {
        Self {
            slines: vec![],
            styles: vec![],
            filters: vec![],

            // This is not the default libass uses (which is 324x288),
            // but it seems like a reasonable default for a modern age.
            playback_resolution: Resolution { x: 1920, y: 1080 },
        }
    }
}
