//! This module contains types for Samaku's internal representation
//! of subtitles, as well as the logic for compiling them to ASS
//! ones.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{Add, Index, IndexMut, Range, Sub};

pub use emit::emit;
pub use emit::emit_timecode;

pub use event_track::*;

pub use import::import;

use crate::nde::tags::{
    Alignment, Colour, HorizontalAlignment, Transparency, VerticalAlignment, WrapStyle,
};
use crate::{media, message, model, nde, style};

pub mod compile;
mod emit;
mod event_track;
mod import;
pub mod parse;
mod uu;

/// A subtitle event, i.e. an individual “subtitle”.
///
/// “Event” is the unambiguous term for a subtitle line, or a typeset sign, or a frame-by-frame
/// or clipped part of a sign. It is shown from a specific start time on for a specific duration,
/// contains text and override tags, and certain other metadata.
///
/// samaku uses events in two main forms: ones that own their data (usually `Event<'static>`)
/// stored in the global state, and derived ones that will reference the data in some other event,
/// for example as a result of compilation.
///
/// We extend libass' simple model of events with certain extra properties, like references to
/// external “extradata”, most notably NDE filters.
#[derive(Debug, Clone, Default)]
pub struct Event<'a> {
    /// The instant, in milliseconds, when this line first appears.
    pub start: StartTime,

    /// The time in milliseconds for which this event is shown, beginning at the `start` time.
    pub duration: Duration,

    /// The layer index on which this event is shown. Events on layers with higher numbers are
    /// shown above those on layers with lower numbers.
    pub layer_index: i32,

    /// The index of the style used for the event. If no style with this index exists, the default
    /// style (index 0) is used instead, which is guaranteed to always exist.
    pub style_index: usize,

    /// If this event is not manually positioned using `\pos` tags, these margins determine its
    /// offset from the frame border.
    pub margins: Margins,

    /// The text shown for this event, potentially including ASS formatting tags.
    pub text: Cow<'a, str>,

    /// The ASS “Actor”/“Name” field. Has no effect on rendering whatsoever; purely used for
    /// reference when authoring subtitles.
    pub actor: Cow<'a, str>,

    /// The ASS “Effect” field. Certain special values for this field cause different rendering
    /// behaviour in libass, but it may also be used for reference when authoring.
    pub effect: Cow<'a, str>,

    /// The “type” of event this is — most importantly, whether it is a comment or not, but in the
    /// future we may desire to define even more types of events.
    pub event_type: EventType,

    /// Extradata entries referenced by this line. Most notably, this may include a reference to
    /// an NDE filter.
    pub extradata_ids: Vec<ExtradataId>,
}

impl Event<'_> {
    #[must_use]
    pub fn end(&self) -> StartTime {
        StartTime(self.start.0 + self.duration.0)
    }

    #[must_use]
    pub fn time_range(&self) -> Range<StartTime> {
        self.start..self.end()
    }

    #[must_use]
    pub fn is_comment(&self) -> bool {
        matches!(self.event_type, EventType::Comment)
    }

    /// Unassigns the NDE filter from this event, if one is assigned, returning its ID.
    /// Otherwise, nothing will happen and `None` will be returned.
    ///
    /// # Panics
    /// Panics if the event somehow has multiple filters assigned.
    pub fn unassign_nde_filter(&mut self, extradata: &Extradata) -> Option<ExtradataId> {
        let mut old_id = None;
        self.extradata_ids.retain(|id| {
            if matches!(extradata[*id], ExtradataEntry::NdeFilter(_)) {
                assert!(old_id.is_none(), "Event has multiple assigned NDE filters");
                old_id = Some(*id);
                false
            } else {
                true
            }
        });
        old_id
    }

    /// Unassigns the NDE filter with the given ID from this event, if it is assigned,
    /// returning `true`. Otherwise, nothing will happen and `false` will be returned.
    pub fn unassign_nde_filter_by_id(&mut self, id_to_unassign: ExtradataId) -> bool {
        let len = self.extradata_ids.len();
        self.extradata_ids.retain(|id| *id != id_to_unassign);
        self.extradata_ids.len() < len
    }

    /// Assign an NDE filter to this event, unassigning the previously assigned filter, if one
    /// existed. In that case, returns the ID of that filter.
    pub fn assign_nde_filter(
        &mut self,
        id: ExtradataId,
        extradata: &Extradata,
    ) -> Option<ExtradataId> {
        let old = self.unassign_nde_filter(extradata);
        self.extradata_ids.push(id);
        old
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum EventType {
    #[default]
    Dialogue,
    Comment,
}

impl EventType {
    #[must_use]
    pub fn is_comment(self) -> bool {
        matches!(self, EventType::Comment)
    }
}

/// The time at which an element starts to be shown, in milliseconds.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct StartTime(pub i64);

impl StartTime {
    #[must_use]
    pub fn stab(self) -> Range<StartTime> {
        self..StartTime(self.0 + 1)
    }

    /// Fixed-width: `hh:mm:ss.mmm`.
    #[must_use]
    pub fn format_long(self) -> String {
        let (sign, hours, minutes, seconds, millis) = self.split(false);
        format!("{sign}{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
    }

    /// Compact: `h[:mm]:ss[.m..]` — trims leading hour when 0 and
    /// trims trailing zeros from fractional seconds.
    #[must_use]
    pub fn format_short(self) -> String {
        let (sign, hours, minutes, seconds, millis) = self.split(false);

        let mut result = if hours > 0 {
            format!("{sign}{hours}:{minutes:02}:{seconds:02}")
        } else {
            format!("{sign}{minutes}:{seconds:02}")
        };

        if millis > 0 {
            // fractional seconds with trimmed trailing zeros
            let mut frac = format!("{millis:03}");
            while frac.ends_with('0') {
                frac.pop();
            }
            result.push('.');
            result.push_str(&frac);
        }

        result
    }

    fn split(self, minus: bool) -> (&'static str, u64, u32, u32, u32) {
        if self.0 < 0 {
            return Self(-self.0).split(true);
        }

        #[expect(clippy::cast_sign_loss, reason = "clamped to 0")]
        let ms_total = self.0 as u128;

        #[expect(clippy::cast_possible_truncation, reason = "divided first")]
        let hours = (ms_total / 3_600_000) as u64;
        let minutes = ((ms_total / 60_000) % 60) as u32;
        let seconds = ((ms_total / 1_000) % 60) as u32;
        let millis = (ms_total % 1_000) as u32;

        let sign = if minus { "-" } else { "" };

        (sign, hours, minutes, seconds, millis)
    }
}

impl Add<Duration> for StartTime {
    type Output = StartTime;

    fn add(self, rhs: Duration) -> Self::Output {
        StartTime(self.0 + rhs.0)
    }
}

impl Sub<Duration> for StartTime {
    type Output = StartTime;

    fn sub(self, rhs: Duration) -> Self::Output {
        StartTime(self.0 - rhs.0)
    }
}

impl Sub<StartTime> for StartTime {
    type Output = Duration;

    fn sub(self, rhs: StartTime) -> Self::Output {
        Duration(self.0 - rhs.0)
    }
}

/// The duration for which an element is shown, in milliseconds.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Duration(pub i64);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Angle(pub f64);

/// 1.0 represents 100%.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub x: f64,
    pub y: f64,
}

impl Scale {
    pub const UNIT: Scale = Scale { x: 1.0, y: 1.0 };
}

impl Default for Scale {
    fn default() -> Self {
        Scale::UNIT
    }
}

/// Element- or style-specific left, right, and vertical margins
/// in pixels, corresponding to ASS `MarginL` etc.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
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

/// Converts a libass-style (RGBT — most signficant byte is the red channel, least significant byte
/// is the transparency) 32-bit packed colour into a pair of [`Colour`] and [`Transparency`].
#[must_use]
pub fn unpack_colour_and_transparency_rgbt(packed: u32) -> (Colour, Transparency) {
    #[expect(clippy::unreadable_literal, reason = "readable color literal")]
    let colour = Colour {
        red: ((packed & 0xff000000) >> 24) as u8,
        green: ((packed & 0x00ff0000) >> 16) as u8,
        blue: ((packed & 0x0000ff00) >> 8) as u8,
    };
    #[expect(clippy::unreadable_literal, reason = "readable color literal")]
    #[expect(
        clippy::cast_possible_wrap,
        reason = "does not wrap since the value is restricted to 8 bits using bitwise operations"
    )]
    let transparency = Transparency((packed & 0x000000ff) as i32);

    (colour, transparency)
}

/// Converts an ASS file-style (TBGR — most signficant byte is the transparency, least significant
/// byte is the red channel) 32-bit packed colour into a pair of [`Colour`] and [`Transparency`].
#[must_use]
pub fn unpack_colour_and_transparency_tbgr(packed: u32) -> (Colour, Transparency) {
    #[expect(clippy::unreadable_literal, reason = "readable color literal")]
    let colour = Colour {
        red: (packed & 0x000000ff) as u8,
        green: ((packed & 0x0000ff00) >> 8) as u8,
        blue: ((packed & 0x00ff0000) >> 16) as u8,
    };
    #[expect(clippy::unreadable_literal, reason = "readable color literal")]
    #[expect(
        clippy::cast_possible_wrap,
        reason = "does not wrap since the value is restricted to 8 bits using bitwise operations"
    )]
    let transparency = Transparency(((packed & 0xff000000) >> 24) as i32);

    (colour, transparency)
}

/// Converts a colour and transparency into an RGBT-format (MSB red, LSB transparency)
/// 32-bit integer.
#[must_use]
pub fn pack_colour_and_transparency_rgbt(colour: Colour, transparency: Transparency) -> u32 {
    (u32::from(colour.red) << 24)
        | (u32::from(colour.green) << 16)
        | (u32::from(colour.blue) << 8)
        | u32::from(transparency.rendered())
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

impl std::fmt::Display for JustifyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "Auto"),
            Self::Left => write!(f, "Left"),
            Self::Center => write!(f, "Center"),
            Self::Right => write!(f, "Right"),
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

impl std::fmt::Display for BorderStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "Outline & shadow"),
            Self::OpaqueBox => write!(f, "Opaque box"),
            Self::Background => write!(f, "Background"),
        }
    }
}

/// Subtitle colour mangling mode.
///
/// Needed for colour compatibility with certain old scripts
/// targeting VSFilter etc. For newly created scripts, there is absolutely no reason to use a
/// value other than [`YCbCrMatrix::None`].
///
/// See <https://github.com/libass/libass/blob/5c15c883a4783641f7e71a6a1f440209965eb64f/libass/ass_types.h#L152>
/// for further details.
#[derive(Debug, Clone, Copy, Default)]
pub enum YCbCrMatrix {
    Default = 0,
    Unknown,

    /// Specifies unambiguously that no colour mangling should occur.
    #[default]
    None,

    Bt601Tv,
    Bt601Pc,
    Bt709Tv,
    Bt709Pc,
    Smtpe240MTv,
    Smtpe240MPc,
    FccTv,
    FccPc,
}

/// libass font encoding parameter (corresponding to “Encoding” in styles).
///
/// If this is set to a value other than `1` or `-1`, libass will avoid selecting
/// fonts that lack coverage in the legacy Windows codepage specified by
/// the value.
///
/// See the following libass issue for a detailed explanation:
/// https://github.com/libass/libass/issues/662.
#[derive(Debug, Clone, Copy)]
pub struct FontEncoding(pub i32);

impl FontEncoding {
    /// libass-specific value that supposedly autodetects the required encoding, and also causes
    /// text to be layouted/shaped across override boundaries, which breaks VSFilter compatibility
    /// but is desirable for certain cursive scripts.
    pub const LIBASS_AUTODETECT: FontEncoding = FontEncoding(-1);
}

#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "needed to represent libass' styles in this case"
)]
pub struct Style {
    /// The style's name. Do not modify this value directly; instead, use `StyleList::rename`!
    pub name: String,
    pub font_name: String,

    pub font_size: f64,

    pub primary_colour: Colour,
    pub secondary_colour: Colour,
    pub border_colour: Colour,
    pub shadow_colour: Colour,

    pub primary_transparency: Transparency,
    pub secondary_transparency: Transparency,
    pub border_transparency: Transparency,
    pub shadow_transparency: Transparency,

    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike_out: bool,

    pub scale: Scale,
    pub spacing: f64,
    pub angle: Angle,

    pub border_style: BorderStyle,
    pub border_width: f64,
    pub shadow_distance: f64,

    pub alignment: Alignment,
    pub margins: Margins,

    /// Specify which Windows codepage you require glyph coverage for.
    ///
    /// See the following libass issue for a detailed explanation:
    /// https://github.com/libass/libass/issues/662.
    pub encoding: FontEncoding,

    pub blur: f64,
    pub justify: JustifyMode,
}

impl Style {
    /// To avoid accidentally mutably referring to the name field, we provide this getter method.
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

impl Default for Style {
    /// Samaku's default style.
    fn default() -> Self {
        Self {
            name: "Default".to_owned(),
            font_name: "Barlow".to_owned(),
            font_size: 120.0,
            primary_colour: Colour::WHITE,
            secondary_colour: Colour {
                red: style::SAMAKU_PRIMARY_RED,
                green: style::SAMAKU_PRIMARY_GREEN,
                blue: style::SAMAKU_PRIMARY_BLUE,
            },
            border_colour: Colour::BLACK,
            shadow_colour: Colour::BLACK,
            primary_transparency: Transparency::OPAQUE,
            secondary_transparency: Transparency::OPAQUE,
            border_transparency: Transparency::OPAQUE,
            shadow_transparency: Transparency(128),
            bold: false,
            italic: false,
            underline: false,
            strike_out: false,
            scale: Scale::UNIT,
            spacing: 0.0,
            angle: Angle(0.0),
            border_style: BorderStyle::Default,
            border_width: 5.0,
            shadow_distance: 5.0,
            alignment: Alignment {
                vertical: VerticalAlignment::Sub,
                horizontal: HorizontalAlignment::Center,
            },
            margins: Margins {
                left: 30,
                right: 30,
                vertical: 30,
            },
            encoding: FontEncoding::LIBASS_AUTODETECT,
            blur: 0.0,
            justify: JustifyMode::Auto,
        }
    }
}

/// Collection of styles. Upholds the following guarantees:
/// - There is always at least one style
/// - No two styles have the same name
#[derive(Debug, Clone)]
pub struct StyleList {
    styles: Vec<Style>,
    names: HashMap<String, usize>,
}

impl StyleList {
    #[must_use]
    pub fn new() -> Self {
        let default_style = Style::default();
        let mut default_names = HashMap::new();
        default_names.insert(default_style.name.clone(), 0);

        Self {
            names: default_names,
            styles: vec![default_style],
        }
    }

    /// Creates a new `StyleList` containing the given styles. If there are multiple styles with the
    /// same name, only the last one will be retained (matching libass' lookup behaviour).
    /// Returns the created list, together with the styles that were not inserted because they had
    /// duplicate names, and the mapping of style indices (from the original large list,
    /// to the now smaller list of non-duplicates).
    #[must_use]
    pub fn from_vec(styles: Vec<Style>) -> (Self, StyleLeftovers) {
        let capacity = styles.len();

        if capacity == 0 {
            // Return a list containing a default style
            return (Self::new(), StyleLeftovers::empty(1));
        }

        let mut res = Self {
            names: HashMap::with_capacity(capacity),
            styles: Vec::with_capacity(capacity),
        };
        let mut leftover: Vec<Style> = vec![];
        let mut mapping: Vec<usize> = vec![0_usize; capacity];

        for (orig_index, style) in styles.into_iter().enumerate() {
            let (new_index, old_style) = res.insert(style);

            if let Some(leftover_style) = old_style {
                leftover.push(leftover_style);
            }

            // We can do this since `insert` will always insert a new style with the same name
            // at the same index as the previous style with that name. So the new indices
            // will continue to remain valid even as the style list grows.
            mapping[orig_index] = new_index;
        }

        let leftovers = StyleLeftovers { leftover, mapping };

        (res, leftovers)
    }

    /// Add a new style to the end, if no style with the same name exists already. Otherwise, the
    /// existing style will be replaced. Returns the index of the inserted style together with the
    /// style previously located at that position, if present.
    pub fn insert(&mut self, style: Style) -> (usize, Option<Style>) {
        if let Some(ref_index) = self.names.get(&style.name) {
            let index = *ref_index;
            let old_style = std::mem::replace(&mut self.styles[index], style);
            return (index, Some(old_style));
        }

        let new_index = self.styles.len();
        self.names.insert(style.name.clone(), new_index);
        self.styles.push(style);
        (new_index, None)
    }

    /// Remove a style by index. Returns the style that was removed. All styles with index > `index`
    /// will have their indices shifted down by 1. The caller must take care to
    /// update references within events, both to assign the default style to events that had the
    /// removed style assigned and to shift indices in events that have styles with greater indices
    /// assigned. To assist with this, a `StyleShift` is returned together with the removed style.
    pub fn remove(&mut self, index: usize) -> (Style, StyleShift) {
        let style = self.styles.remove(index);
        self.names.remove(&style.name);
        let shift = StyleShift::Negative { pivot: index };
        self.shift_names(&shift);
        (style, shift)
    }

    /// Insert a style at the given index, resulting in the returned `StyleShift`.
    /// This method should primarily be used for styles that were recently deleted.
    /// In most cases, you would want to use `insert` instead, which gracefully handles
    /// duplicate names and doesn't result in shifts.
    ///
    /// # Panics
    /// Panics if trying to restore a style with a name that already exists.
    pub fn restore(&mut self, index: usize, style: Style) -> StyleShift {
        assert!(
            !self.names.contains_key(style.name()),
            "tried to restore duplicate style"
        );
        self.styles.insert(index, style);
        let shift = StyleShift::Positive { pivot: index };
        self.shift_names(&shift);
        self.names.insert(self.styles[index].name.clone(), index);
        shift
    }

    fn shift_names(&mut self, shift: &StyleShift) {
        let mut set = HashSet::new();
        for index in &mut self.names.values_mut() {
            shift.apply(index, &(), &mut set);
        }
    }

    /// Look up a style by name. Returns the index of the style, if one was found.
    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Option<usize> {
        self.names.get(name).copied()
    }

    /// Change a style's name.
    /// If a style with the same name already exists, the name will be deduplicated
    /// (e.g. "Default" → "Default-1"). In that case, the Method will return `Some(new_name)`.
    /// If the name was not deduplicated, `None` will be returned.
    ///
    /// # Panics
    /// Panics if the current name of the style to be renamed does not match our reference to it,
    /// because it has been renamed “manually” by setting its `name` field in the meantime.
    pub fn rename<S: Into<String>>(&mut self, index: usize, new_name: S) -> Option<String> {
        let original_new_name = new_name.into();
        let mut new_name = original_new_name.clone();
        let mut counter = 0;
        while self.names.contains_key(&new_name) {
            counter += 1;
            new_name = format!("{original_new_name}-{counter}");
        }

        let old_name = std::mem::replace(&mut self.styles[index].name, new_name.clone());
        assert_eq!(
            self.names.remove(&old_name),
            Some(index),
            "Style index did not match expected value in `rename` — was a style manually renamed by changing its `name` field?"
        );
        self.names.insert(new_name.clone(), index);

        (counter > 0).then_some(new_name)
    }

    #[must_use]
    #[expect(
        clippy::len_without_is_empty,
        reason = "no point in an `is_empty` method since `StyleList` is guaranteed to never be empty"
    )]
    pub fn len(&self) -> usize {
        self.styles.len()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Style] {
        &self.styles
    }
}

impl Default for StyleList {
    fn default() -> Self {
        Self::new()
    }
}

impl Index<usize> for StyleList {
    type Output = Style;

    fn index(&self, index: usize) -> &Self::Output {
        &self.styles[index]
    }
}

impl IndexMut<usize> for StyleList {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.styles[index]
    }
}

/// Utility for shifting style indices for events when styles are deleted or restored.
pub enum StyleShift {
    Negative { pivot: usize },
    Positive { pivot: usize },
}

impl StyleShift {
    /// Applies this `StyleShift` to the given index, shifting it to match the updated style indices.
    /// If the style with the given index would be deleted, the given `entry` is inserted into the given
    /// `HashSet`.
    /// If a style would be created and the given `entry` is already contained
    /// in the given `HashSet`, the given index is set to the index of the new style.
    pub fn apply<T>(&self, index: &mut usize, entry: &T, collect: &mut HashSet<T>)
    where
        T: Clone + Eq + Hash,
    {
        match self {
            Self::Negative { pivot } => {
                match (*index).cmp(pivot) {
                    std::cmp::Ordering::Less => {}
                    std::cmp::Ordering::Equal => {
                        // Reset to default style, and collect the entry.
                        collect.insert(entry.clone());
                        *index = 0;
                    }
                    std::cmp::Ordering::Greater => *index -= 1,
                }
            }
            Self::Positive { pivot } => {
                match (*index).cmp(pivot) {
                    std::cmp::Ordering::Less => {}
                    std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => *index += 1,
                }

                // Restore entries in collection to the new style
                if collect.contains(entry) {
                    *index = *pivot;
                }
            }
        }
    }
}

/// Represents data that was left over after deduplicating the style list.
pub struct StyleLeftovers {
    /// Remaining duplicate styles that would be inaccessible in libass.
    pub leftover: Vec<Style>,

    /// The mapping of input style indices to output style indices, after deduplicating.
    pub mapping: Vec<usize>,
}

impl StyleLeftovers {
    fn empty(num_styles: usize) -> Self {
        Self {
            leftover: vec![],
            mapping: (0..num_styles).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScriptInfo {
    pub wrap_style: WrapStyle,
    pub scaled_border_and_shadow: bool,
    pub kerning: bool,
    pub timer: f64,
    pub ycbcr_matrix: YCbCrMatrix,
    pub playback_resolution: Resolution,
    pub extra_info: HashMap<String, String>,
}

impl Default for ScriptInfo {
    fn default() -> Self {
        Self {
            wrap_style: WrapStyle::SmartEven,
            scaled_border_and_shadow: true,
            kerning: true,
            timer: 0.0,
            ycbcr_matrix: YCbCrMatrix::None,

            // This is not the default libass uses (which is 324x288),
            // but it seems like a reasonable default for a modern age.
            playback_resolution: Resolution { x: 1920, y: 1080 },

            extra_info: HashMap::new(),
        }
    }
}

/// Represents all data that can be contained within an `.ass` file.
#[derive(Debug, Clone)]
pub struct File {
    /// Metadata, containing information like the playback resolution, the YCbCr matrix, etc.
    pub script_info: ScriptInfo,

    /// Aegisub-specific metadata, from the “Aegisub Project Garbage” section. Currently this is
    /// unused, but in the future we might use it for more compatibility with Aegisub, like loading
    /// (dummy) videos or keeping track of the currently selected event in the same way.
    pub aegi_metadata: HashMap<String, String>,

    /// Binary files (fonts or graphics) attached to the subtitles. Currently unused within samaku.
    /// This feature is pretty obscure anyway, we might eventually decide to get rid of these again.
    pub attachments: Vec<Attachment>,

    /// Other sections that were not recognised by the parser. They are represented here opaquely
    /// (as `key => [lines]`) to avoid removing them, in case for example some Aegisub variant
    /// decides to introduce a new section.
    pub other_sections: HashMap<String, Vec<String>>,

    /// Base styles for the events. This is wrapped in a `Trace` because if it gets modified,
    /// certain iced widgets need to be notified about this.
    pub styles: model::Trace<StyleList>,

    /// The events, i.e. the individual subtitle lines.
    pub events: EventTrack,

    /// Additional arbitrary data which may be referred to by events. Importantly for samaku's
    /// purposes, this includes NDE filters, but there may also be other data such as
    /// Aegisub-specific properties.
    pub extradata: Extradata,
}

impl File {
    /// Parse the given stream of lines into a [`File`] with a list of non-fatal parse warnings.
    ///
    /// # Errors
    /// Errors when the stream returns an IO error, or when an unrecoverable parse error is encountered.
    /// The parser is quite tolerant, so this should not happen often.
    ///
    /// # Panics
    /// Panics if there are more styles than would fit into an `i32`.
    pub async fn parse<R: smol::io::AsyncBufRead + Unpin>(
        input: smol::io::Lines<R>,
    ) -> Result<(File, Vec<parse::Warning>), parse::SubtitleParseError> {
        parse::parse(input).await
    }

    /// Create a new `File` from the given libass `OpaqueTrack`.
    /// Returns the `File` and the list of leftover duplicate (and thus unused) styles.
    #[must_use]
    pub fn from_opaque(opaque: &media::subtitle::OpaqueTrack) -> (Self, Vec<Style>) {
        let (style_list, leftovers) = StyleList::from_vec(opaque.styles());
        let StyleLeftovers { leftover, mapping } = leftovers;

        // We need to remap the style indices libass assigned
        // to our new ones after deduplicating/potentially reordering
        // the style list.
        let mut events = opaque.to_event_track();
        for event in events.iter_events_mut() {
            event.style_index = mapping[event.style_index];
        }

        let new_file = Self {
            events,
            styles: model::Trace::new(style_list),
            script_info: opaque.script_info(),
            ..Default::default()
        };

        (new_file, leftover)
    }
}

impl Default for File {
    fn default() -> Self {
        Self {
            script_info: ScriptInfo::default(),
            aegi_metadata: HashMap::new(),
            attachments: vec![],
            other_sections: HashMap::new(),
            styles: model::Trace::new(StyleList::default()),
            events: EventTrack::default(),
            extradata: Extradata::default(),
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ExtradataId(u32);

#[derive(Debug, Clone, Default)]
pub struct Extradata {
    entries: BTreeMap<ExtradataId, ExtradataEntry>,
    next_id: ExtradataId,
}

pub type IterFilters<'a> = std::iter::FilterMap<
    std::collections::btree_map::Iter<'a, ExtradataId, ExtradataEntry>,
    fn((&'a ExtradataId, &'a ExtradataEntry)) -> Option<(ExtradataId, &'a nde::Filter)>,
>;

impl Extradata {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a new extradata entry. Returns the newly created ID of the appended entry.
    pub fn push(&mut self, entry: ExtradataEntry) -> ExtradataId {
        let new_id = self.next_id;
        self.entries.insert(new_id, entry);
        self.next_id = ExtradataId(new_id.0 + 1);

        new_id
    }

    /// Append a new filter. Returns the newly created extradata ID.
    pub fn push_filter(&mut self, filter: nde::Filter) -> ExtradataId {
        self.push(ExtradataEntry::NdeFilter(filter))
    }

    /// Inserts an extradata entry at the given ID.
    pub fn insert(&mut self, id: ExtradataId, entry: ExtradataEntry) {
        self.entries.insert(id, entry);
    }

    /// Inserts a filter at the given ID.
    pub fn insert_filter(&mut self, id: ExtradataId, filter: nde::Filter) {
        self.insert(id, ExtradataEntry::NdeFilter(filter));
    }

    /// Remove an entry. The caller must take care to remove references to it from events!
    pub fn remove(&mut self, id: ExtradataId) -> Option<ExtradataEntry> {
        self.entries.remove(&id)
    }

    /// Iterate over all existing NDE filters with their indices.
    pub fn iter_filters(&'_ self) -> IterFilters<'_> {
        #[expect(
            clippy::match_wildcard_for_single_variants,
            reason = "the wildcard match should cover all other extradata types that may be added in the future as well"
        )]
        self.entries
            .iter()
            .filter_map(|(index, entry)| match entry {
                ExtradataEntry::NdeFilter(filter) => Some((*index, filter)),
                _ => None,
            })
    }

    /// Returns the assigned NDE filter for a given event, if one exists.
    #[must_use]
    pub fn nde_filter_for_event<'a>(&'a self, event: &Event) -> Option<&'a nde::Filter> {
        for extradata_id in &event.extradata_ids {
            if let ExtradataEntry::NdeFilter(filter) = &self[*extradata_id] {
                return Some(filter);
            }
        }

        None
    }

    /// Returns the assigned NDE filter for a given event, if one exists, together with its ID.
    #[must_use]
    pub fn nde_filter_and_id_for_event<'a>(
        &'a self,
        event: &Event,
    ) -> Option<(ExtradataId, &'a nde::Filter)> {
        for extradata_id in &event.extradata_ids {
            if let ExtradataEntry::NdeFilter(filter) = &self[*extradata_id] {
                return Some((*extradata_id, filter));
            }
        }

        None
    }

    /// Get a mutable reference to the NDE filter assigned to the given event, if one is assigned.
    ///
    /// # Panics
    /// This function should never panic in safe operation.
    #[must_use]
    pub fn nde_filter_for_event_mut<'a>(
        &'a mut self,
        event: &Event,
    ) -> Option<&'a mut nde::Filter> {
        // We have to implement it in this roundabout way because of borrow checker limitations;
        // if we simply return the filter reference in the loop, the borrow checker cannot prove
        // that the mutable reference is unique.
        let mut maybe_filter_id: Option<ExtradataId> = None;
        for extradata_id in &event.extradata_ids {
            if let ExtradataEntry::NdeFilter(_) = &self[*extradata_id] {
                maybe_filter_id = Some(*extradata_id);
                break;
            }
        }

        let filter_id = maybe_filter_id?;

        let ExtradataEntry::NdeFilter(filter) = &mut self[filter_id] else {
            panic!();
        };

        Some(filter)
    }

    /// Dispatch message to node.
    pub fn update_node(
        &mut self,
        filter_index: ExtradataId,
        node_index: nde::graph::NodeId,
        message: message::Node,
    ) -> anyhow::Result<()> {
        let node = self.get_node(filter_index, node_index)?;
        node.node.update(message)
    }

    /// Notify a node that a reticule has been moved.
    pub fn reticule_update(
        &mut self,
        reticules: &mut model::reticule::Reticules,
        reticule_index: model::reticule::Index,
        position: nde::tags::Position,
    ) -> anyhow::Result<nde::tags::Position> {
        let node = self.get_node(reticules.source_filter_index, reticules.source_node_index)?;
        node.node
            .reticule_update(reticules, reticule_index, position)
    }

    fn get_node(
        &mut self,
        filter_index: ExtradataId,
        node_index: nde::graph::NodeId,
    ) -> anyhow::Result<&mut nde::graph::VisualNode> {
        let Some(entry) = self.entries.get_mut(&filter_index) else {
            anyhow::bail!("Extradata entry does not exist at index {}", filter_index.0);
        };
        let ExtradataEntry::NdeFilter(filter) = entry else {
            anyhow::bail!("Extradata at index {} is not an NDE filter", filter_index.0);
        };
        let Some(node) = filter.graph.nodes.get_mut(node_index.0) else {
            anyhow::bail!("Could not find node at index {}", node_index.0);
        };

        Ok(node)
    }
}

impl Index<ExtradataId> for Extradata {
    type Output = ExtradataEntry;

    fn index(&self, index: ExtradataId) -> &ExtradataEntry {
        self.entries
            .get(&index)
            .unwrap_or_else(|| panic!("Tried to get non-existent extradata entry with {index:?}"))
    }
}

impl IndexMut<ExtradataId> for Extradata {
    fn index_mut(&mut self, index: ExtradataId) -> &mut ExtradataEntry {
        self.entries.get_mut(&index).unwrap_or_else(|| {
            panic!("Tried to get_mut non-existent extradata entry with {index:?}")
        })
    }
}

#[derive(Debug, Clone)]
pub enum ExtradataEntry {
    NdeFilter(nde::Filter),
    Opaque { key: String, value: Vec<u8> },
}

impl ExtradataEntry {
    /// Asserts that this extradata entry is a filter,
    /// and returns the contained filter graph.
    ///
    /// # Panics
    /// Panics if this extradata entry is not a filter.
    #[must_use]
    pub fn assert_filter(&self) -> &nde::Filter {
        if let ExtradataEntry::NdeFilter(filter) = self {
            filter
        } else {
            panic!("assert_filter() failed, instead found: {self:?}");
        }
    }

    /// Asserts that this extradata entry is a filter,
    /// and mutably returns the contained filter graph.
    ///
    /// # Panics
    /// Panics if this extradata entry is not a filter.
    #[must_use]
    pub fn assert_filter_mut(&mut self) -> &mut nde::Filter {
        if let ExtradataEntry::NdeFilter(filter) = self {
            filter
        } else {
            panic!("assert_filter() failed, instead found: {self:?}");
        }
    }
}

#[derive(Debug, Clone)]
pub struct Attachment {
    attachment_type: AttachmentType,
    filename: String,
    uu_data: String,
}

impl Attachment {
    /// Decode the UU-encoded data contained within this attachment to raw binary data.
    ///
    /// # Errors
    /// Returns a `DecodeError` if the contained data is invalid.
    pub fn decode(&self) -> Result<Vec<u8>, data_encoding::DecodeError> {
        uu::decode(&self.uu_data)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentType {
    Font,
    Graphic,
}

#[cfg(test)]
mod tests {
    use assert_matches2::assert_matches;
    use smol::io::AsyncBufReadExt as _;
    use std::path::Path;

    use super::*;
    use crate::{subtitle, test_utils::test_file};

    fn import(path: &Path) -> (File, Vec<Style>) {
        media::subtitle::set_libass_test_callback();
        let content = smol::block_on(async { subtitle::import(path).await }).unwrap();
        let opaque = media::subtitle::OpaqueTrack::parse(&content);
        File::from_opaque(&opaque)
    }

    #[test]
    fn format_times() {
        assert_eq!(StartTime(0).format_short(), "0:00");
        assert_eq!(StartTime(0).format_long(), "00:00:00.000");

        assert_eq!(StartTime(1000).format_short(), "0:01");
        assert_eq!(StartTime(1000).format_long(), "00:00:01.000");

        assert_eq!(StartTime(1500).format_short(), "0:01.5");
        assert_eq!(StartTime(1500).format_long(), "00:00:01.500");

        assert_eq!(StartTime(1501).format_short(), "0:01.501");
        assert_eq!(StartTime(1501).format_long(), "00:00:01.501");

        assert_eq!(StartTime(601_500).format_short(), "10:01.5");
        assert_eq!(StartTime(601_500).format_long(), "00:10:01.500");

        assert_eq!(StartTime(3_601_500).format_short(), "1:00:01.5");
        assert_eq!(StartTime(3_601_500).format_long(), "01:00:01.500");

        assert_eq!(StartTime(36_001_500).format_short(), "10:00:01.5");
        assert_eq!(StartTime(36_001_500).format_long(), "10:00:01.500");

        assert_eq!(StartTime(360_001_500).format_short(), "100:00:01.5");
        assert_eq!(StartTime(360_001_500).format_long(), "100:00:01.500");

        assert_eq!(StartTime(-1500).format_short(), "-0:01.5");
        assert_eq!(StartTime(-1500).format_long(), "-00:00:01.500");
    }

    #[test]
    fn style_list() {
        let mut style_list = StyleList::new();
        assert_eq!(style_list.len(), 1);

        let (index, result) = style_list.insert(Style {
            name: "a".to_owned(),
            bold: true,
            ..Default::default()
        });
        assert_eq!(index, 1);
        assert_matches!(result, None);

        let (index, result) = style_list.insert(Style {
            name: "b".to_owned(),
            italic: true,
            ..Default::default()
        });
        assert_eq!(index, 2);
        assert_matches!(result, None);

        let (index, result) = style_list.insert(Style {
            name: "a".to_owned(),
            bold: false,
            ..Default::default()
        });
        assert_eq!(index, 1);
        assert_matches!(result, Some(old_style));
        assert!(old_style.bold);

        let maybe_index = style_list.find_by_name("b");
        assert_matches!(maybe_index, Some(b_index));
        assert!(style_list[b_index].italic);

        style_list.rename(2, "c");
        assert_eq!(style_list[2].name(), "c");

        let maybe_index = style_list.find_by_name("c");
        assert_matches!(maybe_index, Some(c_index));
        assert!(style_list[c_index].italic);
    }

    #[test]
    fn attachment_decode() {
        let path = test_file("test_files/extra_sections.ass");
        let (ass_file, _warnings) = parse::tests::parse_blocking(&path);

        assert_eq!(ass_file.attachments.len(), 1);
        let at1 = &ass_file.attachments[0];
        assert_eq!(at1.attachment_type, AttachmentType::Graphic);

        let source_data = std::fs::read(test_file("test_files/4x4.jpg")).unwrap();
        let decoded = at1.decode().unwrap();
        assert_eq!(decoded, source_data);
    }

    #[test]
    fn extradata_round_trip() {
        const SHORT_VALUE: &[u8] = b"\x00123456789";
        const LONG_VALUE: &[u8] = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09";

        let mut entries = BTreeMap::new();
        entries.insert(
            ExtradataId(0),
            ExtradataEntry::Opaque {
                key: "short".to_owned(),
                value: SHORT_VALUE.to_vec(),
            },
        );
        entries.insert(
            ExtradataId(1),
            ExtradataEntry::Opaque {
                key: "long".to_owned(),
                value: LONG_VALUE.to_vec(),
            },
        );

        let ass_file = File {
            extradata: Extradata {
                entries,
                next_id: ExtradataId(2),
            },
            ..Default::default()
        };

        let mut emitted = String::new();
        emit(&mut emitted, &ass_file, None).unwrap();

        // Make sure the short one was inline-encoded and the long one UU-encoded
        assert!(emitted.contains("short,e#00"));
        assert!(emitted.contains("long,u!"));

        let (parsed, _warnings) = smol::block_on(async {
            File::parse(smol::io::BufReader::new(emitted.as_bytes()).lines()).await
        })
        .unwrap();

        assert_eq!(parsed.extradata.entries.len(), 2);
        assert_eq!(parsed.extradata.next_id, ExtradataId(2));

        let e0 = &parsed.extradata.entries[&ExtradataId(0)];
        let e1 = &parsed.extradata.entries[&ExtradataId(1)];

        assert_matches!(e0, ExtradataEntry::Opaque { key: k0, value: v0 });
        assert_matches!(e1, ExtradataEntry::Opaque { key: k1, value: v1 });

        assert_eq!(k0, "short");
        assert_eq!(v0, SHORT_VALUE);
        assert_eq!(k1, "long");
        assert_eq!(v1, LONG_VALUE);
    }

    #[test]
    fn opaque_sections_round_trip() {
        let path = test_file("test_files/opaque_sections.ass");
        let (ass_file, _warnings) = parse::tests::parse_blocking(&path);

        assert_eq!(ass_file.other_sections.len(), 1);
        assert_matches!(ass_file.other_sections.get("Croutons Recipe"), Some(recipe));
        assert!(recipe[0].contains("Olive Oil"));

        let mut emitted = String::new();
        emit(&mut emitted, &ass_file, None).unwrap();

        assert!(emitted.contains("[Croutons Recipe]"));
        assert!(emitted.contains("Olive Oil"));
        assert!(emitted.contains("Pepper\nStep"));
    }

    #[test]
    fn emit_export() {
        // Create a File containing a single event that specifies an NDE filter which splits it
        // frame-by-frame, and ensure that the exported output contains the correct number of
        // resulting events.

        let mut graph =
            nde::Graph::from_single_intermediate(Box::new(nde::node::SplitFrameByFrame {}));

        // The SplitFrameByFrame node requires a frame rate, so add a respective input node and
        // connect it
        graph.nodes.push(nde::graph::VisualNode {
            node: Box::new(nde::node::InputFrameRate {}),
            position: iced::Point::new(0.0, 200.0),
        });
        graph.connect(
            nde::graph::PreviousEndpoint {
                node_index: nde::graph::NodeId(3),
                socket_index: nde::graph::SocketId(0),
            },
            nde::graph::NextEndpoint {
                node_index: nde::graph::NodeId(1),
                socket_index: nde::graph::SocketId(1),
            },
        );

        let filter = nde::Filter {
            name: "foo".to_owned(),
            graph,
        };

        let mut extradata = Extradata::new();
        extradata.push(ExtradataEntry::NdeFilter(filter));

        let event = Event {
            start: StartTime(0),
            duration: Duration(999),
            extradata_ids: vec![ExtradataId(0)],
            ..Default::default()
        };

        let ass_file = File {
            events: vec![event].into_iter().collect(),
            extradata,
            ..Default::default()
        };

        let context = compile::Context {
            frame_rate: media::FrameRate {
                numerator: 24,
                denominator: 1,
            },
        };
        let mut emitted = String::new();
        emit(&mut emitted, &ass_file, Some(context)).unwrap();

        let (parsed, _warnings) = parse::tests::parse_str(&emitted);
        assert_eq!(parsed.events.len(), 24);
    }

    #[test]
    fn compile_comments() {
        // Test that comments are skipped in compilation

        let events: EventTrack = vec![
            Event {
                event_type: EventType::Dialogue,
                duration: Duration(5000),
                ..Default::default()
            },
            Event {
                event_type: EventType::Comment,
                duration: Duration(5000),
                ..Default::default()
            },
        ]
        .into_iter()
        .collect();

        let context = compile::Context {
            frame_rate: media::FrameRate {
                numerator: 24,
                denominator: 1,
            },
        };
        let compiled = events.compile_all(&Extradata::default(), &context);

        assert_eq!(compiled.len(), 1);
    }

    #[test]
    fn duplicate_style_handling() {
        // -- import --
        let path = test_file("test_files/duplicate_styles.ass");
        let (file, leftover) = import(&path);

        assert_eq!(leftover.len(), 2);
        assert_eq!(leftover[0].name(), "Default"); // duplicate Default style with the one libass creates by itself
        assert_eq!(leftover[1].name(), "New style 1");
        assert_eq!(file.styles.len(), 2);

        assert_eq!(
            file.events.nth(1).1.style_index,
            file.events.nth(2).1.style_index
        );

        // verify that this is the default style
        let first_event_style = &file.styles[file.events.nth(0).1.style_index];
        assert_eq!(first_event_style.name(), "Default");

        // should be the later one which is bold
        let second_event_style = &file.styles[file.events.nth(1).1.style_index];
        assert!(second_event_style.bold);

        // -- new file --
        let mut file = File::default();

        let style_1 = Style {
            name: "Style 1".to_owned(),
            bold: true,
            ..Default::default()
        };
        let style_2 = Style {
            name: "Style 2".to_owned(),
            italic: true,
            ..Default::default()
        };

        let (index, leftover) = file.styles.insert(style_1);
        assert_matches!(leftover, None);
        assert_eq!(index, 1);

        let (index, leftover) = file.styles.insert(style_2);
        assert_matches!(leftover, None);
        assert_eq!(index, 2);

        // creating a duplicate style by renaming should keep both duplicates internally
        let renamed = file.styles.rename(index, "Style 1");
        assert_eq!(file.styles.len(), 3);

        // ... but the new style should now have a different name
        assert_ne!(file.styles[index].name, "Style 1");
        assert_eq!(file.styles[index].name, renamed.expect("not renamed"));
    }

    #[test]
    #[should_panic(expected = "tried to restore duplicate style")]
    fn duplicate_style_handling_restore() {
        let mut list = StyleList::new();
        list.restore(1, Style::default());
    }

    #[test]
    fn style_list_name_mapping() {
        let mut list = StyleList::new();
        // "Default" is at index 0 after new()

        let (idx_a, _) = list.insert(Style {
            name: "a".to_owned(),
            bold: true,
            ..Default::default()
        });
        let (idx_b, _) = list.insert(Style {
            name: "b".to_owned(),
            italic: true,
            ..Default::default()
        });
        let (idx_c, _) = list.insert(Style {
            name: "c".to_owned(),
            ..Default::default()
        });

        assert_eq!(idx_a, 1);
        assert_eq!(idx_b, 2);
        assert_eq!(idx_c, 3);
        assert_eq!(list.find_by_name("a"), Some(1));
        assert_eq!(list.find_by_name("b"), Some(2));
        assert_eq!(list.find_by_name("c"), Some(3));

        // Remove "b" at index 2; "a" unchanged, "c" shifts down
        let (removed, _shift) = list.remove(idx_b);
        assert_eq!(removed.name, "b");

        assert_eq!(list.find_by_name("Default"), Some(0));
        assert_eq!(list.find_by_name("a"), Some(1));
        assert_eq!(list.find_by_name("b"), None);
        assert_eq!(list.find_by_name("c"), Some(2));

        // Restore "b" at index 2; "c" shifts back up
        list.restore(2, removed);

        assert_eq!(list.find_by_name("Default"), Some(0));
        assert_eq!(list.find_by_name("a"), Some(1));
        assert_eq!(list.find_by_name("b"), Some(2));
        assert_eq!(list.find_by_name("c"), Some(3));
    }

    #[test]
    fn shift_style_indices() {
        let neg = StyleShift::Negative { pivot: 2 };
        let mut collect: HashSet<usize> = HashSet::new();

        let mut idx: usize = 0;
        neg.apply(&mut idx, &0, &mut collect);
        assert_eq!(idx, 0); // unchanged: 0 < pivot

        let mut idx: usize = 1;
        neg.apply(&mut idx, &1, &mut collect);
        assert_eq!(idx, 1); // unchanged: 1 < pivot

        let mut idx: usize = 2;
        neg.apply(&mut idx, &2, &mut collect);
        assert_eq!(idx, 0); // reset to default: == pivot
        assert!(collect.contains(&2)); // entry collected

        let mut idx: usize = 3;
        neg.apply(&mut idx, &3, &mut collect);
        assert_eq!(idx, 2); // decremented: 3 > pivot

        // collect now contains {2}
        let pos = StyleShift::Positive { pivot: 2 };

        let mut idx: usize = 0;
        pos.apply(&mut idx, &0, &mut collect);
        assert_eq!(idx, 0); // unchanged: 0 < pivot, not in collect

        let mut idx: usize = 1;
        pos.apply(&mut idx, &1, &mut collect);
        assert_eq!(idx, 1); // unchanged: 1 < pivot, not in collect, &mut collect

        let mut idx: usize = 2; // was originally 3, shifted down to 2
        pos.apply(&mut idx, &3, &mut collect);
        assert_eq!(idx, 3); // incremented back: 2 >= pivot, 3 not in collect

        let mut idx: usize = 0; // was reset to 0 when its style was deleted
        pos.apply(&mut idx, &2, &mut collect);
        assert_eq!(idx, 2); // restored to pivot: entry 2 is in collect
    }

    #[test]
    fn unpack_rgbt_known_value() {
        // Packed as 0xRRGGBBTT
        let (colour, transparency) = unpack_colour_and_transparency_rgbt(0xFF_00_80_20);
        assert_eq!(
            colour,
            Colour {
                red: 0xFF,
                green: 0x00,
                blue: 0x80
            }
        );
        assert_eq!(transparency, Transparency(0x20));
    }

    #[test]
    fn unpack_tbgr_known_value() {
        // Packed as 0xTTBBGGRR — same logical colour as above, different byte order
        let (colour, transparency) = unpack_colour_and_transparency_tbgr(0x20_80_00_FF);
        assert_eq!(
            colour,
            Colour {
                red: 0xFF,
                green: 0x00,
                blue: 0x80
            }
        );
        assert_eq!(transparency, Transparency(0x20));
    }

    #[test]
    fn pack_unpack_rgbt_round_trip() {
        let colour = Colour {
            red: 0xAB,
            green: 0xCD,
            blue: 0xEF,
        };
        let transparency = Transparency(0x12);
        let packed = pack_colour_and_transparency_rgbt(colour, transparency);
        let (c2, t2) = unpack_colour_and_transparency_rgbt(packed);
        assert_eq!(c2, colour);
        assert_eq!(t2, transparency);
    }

    #[test]
    fn pack_unpack_rgbt_black_opaque() {
        let colour = Colour {
            red: 0,
            green: 0,
            blue: 0,
        };
        let transparency = Transparency(0);
        let packed = pack_colour_and_transparency_rgbt(colour, transparency);
        assert_eq!(packed, 0);
        let (c2, t2) = unpack_colour_and_transparency_rgbt(packed);
        assert_eq!(c2, colour);
        assert_eq!(t2, transparency);
    }

    #[test]
    fn pack_unpack_rgbt_white_fully_transparent() {
        let colour = Colour {
            red: 0xFF,
            green: 0xFF,
            blue: 0xFF,
        };
        let transparency = Transparency(0xFF);
        let packed = pack_colour_and_transparency_rgbt(colour, transparency);
        assert_eq!(packed, 0xFF_FF_FF_FF);
        let (c2, t2) = unpack_colour_and_transparency_rgbt(packed);
        assert_eq!(c2, colour);
        assert_eq!(t2, transparency);
    }
}
