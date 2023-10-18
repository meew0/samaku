//! This module contains types for Samaku's internal representation
//! of subtitles, as well as the logic for compiling them to ASS
//! ones.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::ops::{Index, IndexMut};

pub use emit::emit;

use crate::nde::tags::{
    Alignment, Colour, HorizontalAlignment, Transparency, VerticalAlignment, WrapStyle,
};
use crate::{media, message, nde, style};

pub mod compile;
mod emit;
pub mod parse;

/// An `Sline` (“samaku line”/“subtitle line”/“sign or line”/etc.),
/// in samaku terms, is one conceptual individual “subtitle”,
/// that is, a dialogue line, a complex sign, etc.
/// It may compile to multiple underlying ASS [`Event`]s.
#[derive(Debug, Clone, Default)]
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

    pub actor: String,
    pub effect: String,

    /// Whether this line is a comment or not.
    pub event_type: EventType,

    /// Extradata entries referenced by this line.
    pub extradata_ids: Vec<ExtradataId>,
}

impl Sline {
    #[must_use]
    pub fn end(&self) -> StartTime {
        StartTime(self.start.0 + self.duration.0)
    }

    /// Unassigns the NDE filter from this sline, if one is assigned. Otherwise, nothing will
    /// happen.
    pub fn unassign_nde_filter(&mut self, extradata: &Extradata) {
        self.extradata_ids
            .retain(|id| !matches!(extradata[*id], ExtradataEntry::NdeFilter(_)));
    }

    /// Assign an NDE filter to this sline, unassigning the previously assigned filter, if one
    /// existed.
    pub fn assign_nde_filter(&mut self, id: ExtradataId, extradata: &Extradata) {
        self.unassign_nde_filter(extradata);
        self.extradata_ids.push(id);
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum EventType {
    #[default]
    Dialogue,
    Comment,
}

/// An event in true ASS terms, that is, one subtitle line
/// as it would be found in e.g. Aegisub. Not to be used
/// as the source for anything; only as an intermediate
/// in the conversion to events as used by libass directly
/// (`ASS_Event`)
///
/// See [`Sline`] docs for other fields.
#[derive(Debug, Clone)]
pub struct CompiledEvent<'a> {
    pub start: StartTime,
    pub duration: Duration,
    pub layer_index: i32,
    pub style_index: i32,
    pub margins: Margins,
    pub text: Cow<'a, str>,

    /// Not really clear what this is,
    /// it seems to be used for duplicate checking within libass,
    /// and also potentially for layer-independent Z ordering (?)
    pub read_order: i32,

    /// Name a.k.a. Actor (does nothing)
    pub name: Cow<'a, str>,

    /// Can be used to store arbitrary user data,
    /// but libass also parses this and has some special behaviour
    /// for certain values (e.g. `Banner;`)
    pub effect: Cow<'a, str>,
}

/// The time at which an element starts to be shown, in milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StartTime(pub i64);

/// The duration for which an element is shown, in milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Duration(pub i64);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Angle(pub f64);

/// 1.0 represents 100%
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
    #[allow(clippy::unreadable_literal)]
    let colour = Colour {
        red: ((packed & 0xff000000) >> 24) as u8,
        green: ((packed & 0x00ff0000) >> 16) as u8,
        blue: ((packed & 0x0000ff00) >> 8) as u8,
    };
    #[allow(clippy::unreadable_literal)]
    #[allow(clippy::cast_possible_wrap)]
    let transparency = Transparency((packed & 0x000000ff) as i32);

    (colour, transparency)
}

/// Converts an ASS file-style (TBGR — most signficant byte is the transparency, least significant
/// byte is the red channel) 32-bit packed colour into a pair of [`Colour`] and [`Transparency`].
#[must_use]
pub fn unpack_colour_and_transparency_tbgr(packed: u32) -> (Colour, Transparency) {
    #[allow(clippy::unreadable_literal)]
    let colour = Colour {
        red: (packed & 0x000000ff) as u8,
        green: ((packed & 0x0000ff00) >> 8) as u8,
        blue: ((packed & 0x00ff0000) >> 16) as u8,
    };
    #[allow(clippy::unreadable_literal)]
    #[allow(clippy::cast_possible_wrap)]
    let transparency = Transparency(((packed & 0xff000000) >> 24) as i32);

    (colour, transparency)
}

/// Converts a colour and transparency into an RGBT-format (MSB red, LSB transparency)
/// 32-bit integer.
#[must_use]
pub fn pack_colour_and_transparency_rgbt(colour: Colour, transparency: Transparency) -> u32 {
    u32::from(colour.red) << 24
        | u32::from(colour.green) << 16
        | u32::from(colour.blue) << 8
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

/// See <https://github.com/libass/libass/blob/5c15c883a4783641f7e71a6a1f440209965eb64f/libass/ass_types.h#L152>
#[derive(Debug, Clone, Copy)]
pub enum YCbCrMatrix {
    Default = 0,
    Unknown,
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
/// As far as I can tell, libass only supports two valid values: (`-1` and `0`).
#[derive(Debug, Clone, Copy)]
pub struct FontEncoding(pub i32);

impl FontEncoding {
    /// libass-specific value that supposedly autodetects the required encoding, and also causes
    /// text to be layouted/shaped across override boundaries, which breaks VSFilter compatibility
    /// but is desirable for certain cursive scripts.
    pub const LIBASS_AUTODETECT: FontEncoding = FontEncoding(-1);
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Style {
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

    /// “Windows font charset number”
    /// `-1` = autodetect, Aegisub's default seems to be `1`
    pub encoding: FontEncoding,

    pub blur: f64,
    pub justify: JustifyMode,
}

impl Default for Style {
    /// Samaku's default style.
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            font_name: "Arial".to_string(),
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

/// Ordered collection of [`Sline`]s with associated data.
/// For now, it's just a wrapper around [`Vec`].
/// Might become more advanced in the future.
#[derive(Default)]
pub struct SlineTrack {
    pub slines: Vec<Sline>,
    pub styles: Vec<Style>,
    pub extradata: Extradata,
}

impl SlineTrack {
    /// Returns true if and only if there are no slines in this track
    /// (there may still be some styles)
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slines.is_empty()
    }

    #[must_use]
    pub fn active_sline(&self, active_sline_index: Option<usize>) -> Option<&Sline> {
        match active_sline_index {
            Some(active_sline_index) => Some(&self.slines[active_sline_index]),
            None => None,
        }
    }

    #[must_use]
    pub fn active_sline_mut(&mut self, active_sline_index: Option<usize>) -> Option<&mut Sline> {
        match active_sline_index {
            Some(active_sline_index) => Some(&mut self.slines[active_sline_index]),
            None => None,
        }
    }

    #[must_use]
    pub fn active_nde_filter(&self, active_sline_index: Option<usize>) -> Option<&nde::Filter> {
        match active_sline_index.map(|index| &self.slines[index]) {
            Some(active_sline) => self.extradata.nde_filter_for_sline(active_sline),
            None => None,
        }
    }

    #[must_use]
    pub fn active_nde_filter_mut(
        &mut self,
        active_sline_index: Option<usize>,
    ) -> Option<&mut nde::Filter> {
        match active_sline_index.map(|index| &self.slines[index]) {
            Some(active_sline) => self.extradata.nde_filter_for_sline_mut(active_sline),
            None => None,
        }
    }

    /// Dispatch message to node
    pub fn update_node(
        &mut self,
        active_sline_index: Option<usize>,
        node_index: usize,
        message: message::Node,
    ) {
        if let Some(filter) = self.active_nde_filter_mut(active_sline_index) {
            if let Some(node) = filter.graph.nodes.get_mut(node_index) {
                node.node.update(message);
            }
        }
    }

    /// Compile subtitles in the given frame range to ASS.
    #[must_use]
    pub fn compile(
        &self,
        _frame_start: i32,
        _frame_count: i32,
        frame_rate: media::FrameRate,
    ) -> Vec<CompiledEvent> {
        let mut counter = 0;
        let mut compiled: Vec<CompiledEvent> = vec![];

        for sline in &self.slines {
            match self.extradata.nde_filter_for_sline(sline) {
                Some(filter) => {
                    match compile::nde(sline, &filter.graph, frame_rate, &mut counter) {
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

pub struct AssFile {
    pub script_info: ScriptInfo,
    pub subtitles: SlineTrack,
    pub side_data: SideData,
}

impl AssFile {
    /// Parse the given stream of lines into an [`AssFile`].
    ///
    /// # Errors
    /// Errors when the stream returns an IO error, or when an unrecoverable parse error is encountered.
    /// The parser is quite tolerant, so this should not happen often.
    ///
    /// # Panics
    /// Panics if there are more styles than would fit into an `i32`.
    pub async fn parse(
        input: smol::io::Lines<smol::io::BufReader<smol::fs::File>>,
    ) -> Result<AssFile, parse::Error> {
        parse::parse(input).await
    }
}

#[derive(Default)]
pub struct SideData {
    pub aegi_metadata: HashMap<String, String>,
    pub attachments: Vec<Attachment>,
    pub other_sections: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtradataId(u32);

#[derive(Debug, Default)]
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

    /// Iterate over all existing NDE filters with their indices.
    pub fn iter_filters(&self) -> IterFilters {
        #[allow(clippy::match_wildcard_for_single_variants)]
        self.entries
            .iter()
            .filter_map(|(index, entry)| match entry {
                ExtradataEntry::NdeFilter(filter) => Some((*index, filter)),
                _ => None,
            })
    }

    /// Returns the assigned NDE filter for a given sline, if one exists.
    #[must_use]
    pub fn nde_filter_for_sline(&self, sline: &Sline) -> Option<&nde::Filter> {
        for extradata_id in &sline.extradata_ids {
            if let ExtradataEntry::NdeFilter(filter) = &self[*extradata_id] {
                return Some(filter);
            }
        }

        None
    }

    /// Get a mutable reference to the NDE filter assigned to the given sline, if one is assigned.
    ///
    /// # Panics
    /// This function should never panic in safe operation.
    #[must_use]
    pub fn nde_filter_for_sline_mut(&mut self, sline: &Sline) -> Option<&mut nde::Filter> {
        // We have to implement it in this roundabout way because of borrow checker limitations;
        // if we simply return the filter reference in the loop, the borrow checker cannot prove
        // that the mutable reference is unique.
        let mut maybe_filter_id: Option<ExtradataId> = None;
        for extradata_id in &sline.extradata_ids {
            if let ExtradataEntry::NdeFilter(_) = &self[*extradata_id] {
                maybe_filter_id = Some(*extradata_id);
                break;
            }
        }

        let Some(filter_id) = maybe_filter_id else {
            return None;
        };

        let ExtradataEntry::NdeFilter(filter) = &mut self[filter_id] else {
            panic!();
        };

        Some(filter)
    }
}

impl Index<ExtradataId> for Extradata {
    type Output = ExtradataEntry;

    fn index(&self, id: ExtradataId) -> &ExtradataEntry {
        self.entries
            .get(&id)
            .unwrap_or_else(|| panic!("Tried to get non-existent extradata entry with {id:?}"))
    }
}

impl IndexMut<ExtradataId> for Extradata {
    fn index_mut(&mut self, id: ExtradataId) -> &mut ExtradataEntry {
        self.entries
            .get_mut(&id)
            .unwrap_or_else(|| panic!("Tried to get_mut non-existent extradata entry with {id:?}"))
    }
}

#[derive(Debug)]
pub enum ExtradataEntry {
    NdeFilter(nde::Filter),
    Opaque { key: String, value: String },
}

#[derive(Debug, Clone)]
pub struct Attachment {
    attachment_type: AttachmentType,
    filename: String,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub enum AttachmentType {
    Font,
    Graphic,
}
