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
use crate::{message, model, nde, style};

pub mod compile;
mod emit;
pub mod parse;
mod uu;

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

impl<'a> Event<'a> {
    #[must_use]
    pub fn end(&self) -> StartTime {
        StartTime(self.start.0 + self.duration.0)
    }

    #[must_use]
    pub fn is_comment(&self) -> bool {
        matches!(self.event_type, EventType::Comment)
    }

    /// Unassigns the NDE filter from this event, if one is assigned. Otherwise, nothing will
    /// happen.
    pub fn unassign_nde_filter(&mut self, extradata: &Extradata) {
        self.extradata_ids
            .retain(|id| !matches!(extradata[*id], ExtradataEntry::NdeFilter(_)));
    }

    /// Assign an NDE filter to this event, unassigning the previously assigned filter, if one
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

/// Subtitle colour mangling mode. Needed for colour compatibility with certain old scripts
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

/// libass font encoding parameter (corresponding to “Encoding” in styles). If this is set to a
/// value other than `1` or `-1`, libass will avoid selecting fonts that lack coverage in the
/// legacy Windows codepage specified by the value.
///
/// See the following libass issue for a detailed explanation:
/// https://github.com/libass/libass/issues/662
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
    /// https://github.com/libass/libass/issues/662
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

/// Collection of styles. Upholds the following guarantees:
/// - There is always at least one style
/// - No two styles have the same name
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
    /// duplicate names.
    #[must_use]
    pub fn from_vec(styles: Vec<Style>) -> (Self, Vec<Style>) {
        let capacity = styles.len();

        if capacity == 0 {
            // Return a list containing a default style
            return (Self::new(), vec![]);
        }

        let mut res = Self {
            names: HashMap::with_capacity(capacity),
            styles: Vec::with_capacity(capacity),
        };
        let mut leftover: Vec<Style> = vec![];

        for style in styles {
            if let (_, Some(old_style)) = res.insert(style) {
                leftover.push(old_style);
            };
        }

        (res, leftover)
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

    /// Look up a style by name. Returns the index of the style, if one was found.
    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Option<usize> {
        self.names.get(name).copied()
    }

    /// Change a style's name.
    ///
    /// # Panics
    /// Panics if the current name of the style to be renamed does not match our reference to it,
    /// because it has been renamed “manually” by setting its `name` field in the meantime.
    pub fn rename(&mut self, index: usize, new_name: String) {
        let old_name = std::mem::replace(&mut self.styles[index].name, new_name.clone());
        assert_eq!(self.names.remove(&old_name), Some(index), "Style index did not match expected value in `rename` — was a style manually renamed by changing its `name` field?");
        self.names.insert(new_name, index);
    }

    #[must_use]
    #[allow(clippy::len_without_is_empty)] // no point in an `is_empty` method since `StyleList` is guaranteed to never be empty
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

/// Ordered collection of [`Event`]s.
/// For now, this is just a wrapper around [`Vec`], but in the future it might become more advanced,
/// using a tree-like structure or some time-indexed data structure.
#[derive(Default)]
pub struct EventTrack {
    events: Vec<Event<'static>>,
}

impl EventTrack {
    /// Create a new `EventTrack` from the given `Vec` of events.
    #[must_use]
    pub fn from_vec(events: Vec<Event<'static>>) -> Self {
        Self { events }
    }

    /// Returns true if and only if there are no events in this track.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Returns the number of events in the track.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Event<'static>] {
        self.events.as_slice()
    }

    pub fn push(&mut self, event: Event<'static>) {
        self.events.push(event);
    }

    #[must_use]
    pub fn active_event(&self, active_event_index: Option<usize>) -> Option<&Event<'static>> {
        match active_event_index {
            Some(active_event_index) => Some(&self.events[active_event_index]),
            None => None,
        }
    }

    #[must_use]
    pub fn active_event_mut(
        &mut self,
        active_event_index: Option<usize>,
    ) -> Option<&mut Event<'static>> {
        match active_event_index {
            Some(active_event_index) => Some(&mut self.events[active_event_index]),
            None => None,
        }
    }

    #[must_use]
    pub fn active_nde_filter<'a>(
        &self,
        active_event_index: Option<usize>,
        extradata: &'a Extradata,
    ) -> Option<&'a nde::Filter> {
        match active_event_index.map(|index| &self.events[index]) {
            Some(active_event) => extradata.nde_filter_for_event(active_event),
            None => None,
        }
    }

    #[must_use]
    pub fn active_nde_filter_mut<'a>(
        &self,
        active_event_index: Option<usize>,
        extradata: &'a mut Extradata,
    ) -> Option<&'a mut nde::Filter> {
        match active_event_index.map(|index| &self.events[index]) {
            Some(active_event) => extradata.nde_filter_for_event_mut(active_event),
            None => None,
        }
    }

    /// Dispatch message to node
    pub fn update_node(
        &mut self,
        active_event_index: Option<usize>,
        extradata: &mut Extradata,
        node_index: usize,
        message: message::Node,
    ) {
        if let Some(filter) = self.active_nde_filter_mut(active_event_index, extradata) {
            if let Some(node) = filter.graph.nodes.get_mut(node_index) {
                node.node.update(message);
            }
        }
    }

    /// Compile subtitles in the given frame range to ASS.
    #[must_use]
    pub fn compile<'a>(
        &'a self,
        extradata: &Extradata,
        context: &compile::Context,
        _frame_start: i32,
        _frame_count: Option<i32>,
    ) -> Vec<Event<'a>> {
        let mut compiled: Vec<Event<'a>> = vec![];

        for event in &self.events {
            // Skip comments when compiling events
            if event.is_comment() {
                continue;
            }

            // Run the complex `nde` compilation method if the event has a filter assigned,
            // and the trivial one otherwise
            match extradata.nde_filter_for_event(event) {
                Some(filter) => match compile::nde(event, &filter.graph, context) {
                    Ok(mut nde_result) => match &mut nde_result.events {
                        Some(events) => compiled.append(events),
                        None => println!("No output from NDE filter"),
                    },
                    Err(error) => {
                        println!("Got NdeError while running NDE filter: {error:?}");
                    }
                },
                None => compiled.push(compile::trivial(event)),
            }
        }

        compiled
    }
}

// For now, just transparently pass along `Vec`'s implementation
impl<'a> IntoIterator for &'a EventTrack {
    type Item = &'a Event<'static>;
    type IntoIter = <&'a Vec<Event<'static>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        <&'a Vec<Event<'static>> as IntoIterator>::into_iter(&self.events)
    }
}

impl Index<usize> for EventTrack {
    type Output = Event<'static>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.events[index]
    }
}

impl IndexMut<usize> for EventTrack {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.events[index]
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

/// Represents all data that can be contained within an `.ass` file.
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
    /// Parse the given stream of lines into an [`AssFile`] with a list of non-fatal parse warnings.
    ///
    /// # Errors
    /// Errors when the stream returns an IO error, or when an unrecoverable parse error is encountered.
    /// The parser is quite tolerant, so this should not happen often.
    ///
    /// # Panics
    /// Panics if there are more styles than would fit into an `i32`.
    pub async fn parse<R: smol::io::AsyncBufRead + Unpin>(
        input: smol::io::Lines<R>,
    ) -> Result<(File, Vec<parse::Warning>), parse::Error> {
        parse::parse(input).await
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
    Opaque { key: String, value: Vec<u8> },
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
    use smol::io::AsyncBufReadExt;

    use crate::{media, test_utils::test_file};

    use super::*;

    #[test]
    fn style_list() {
        let mut style_list = StyleList::new();
        assert_eq!(style_list.len(), 1);

        let (index, result) = style_list.insert(Style {
            name: "a".to_string(),
            bold: true,
            ..Default::default()
        });
        assert_eq!(index, 1);
        assert_matches!(result, None);

        let (index, result) = style_list.insert(Style {
            name: "b".to_string(),
            italic: true,
            ..Default::default()
        });
        assert_eq!(index, 2);
        assert_matches!(result, None);

        let (index, result) = style_list.insert(Style {
            name: "a".to_string(),
            bold: false,
            ..Default::default()
        });
        assert_eq!(index, 1);
        assert_matches!(result, Some(old_style));
        assert!(old_style.bold);

        let maybe_index = style_list.find_by_name("b");
        assert_matches!(maybe_index, Some(b_index));
        assert!(style_list[b_index].italic);

        style_list.rename(2, "c".to_string());
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
                key: "short".to_string(),
                value: SHORT_VALUE.to_vec(),
            },
        );
        entries.insert(
            ExtradataId(1),
            ExtradataEntry::Opaque {
                key: "long".to_string(),
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
        emit::emit(&mut emitted, &ass_file, None).unwrap();

        // Make sure the short one was inline-encoded and the long one UU-encoded
        assert!(emitted.contains("short,e#00"));
        assert!(emitted.contains("long,u!"));

        let (parsed, _warnings) = smol::block_on(async {
            File::parse(smol::io::BufReader::new(emitted.as_bytes()).lines()).await
        })
        .unwrap();

        assert_eq!(parsed.extradata.entries.len(), 2);
        assert_eq!(parsed.extradata.next_id, ExtradataId(2));

        let e0 = parsed.extradata.entries.get(&ExtradataId(0)).unwrap();
        let e1 = parsed.extradata.entries.get(&ExtradataId(1)).unwrap();

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
        emit::emit(&mut emitted, &ass_file, None).unwrap();

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
            nde::graph::NextEndpoint {
                node_index: 1,
                socket_index: 1,
            },
            nde::graph::PreviousEndpoint {
                node_index: 3,
                socket_index: 0,
            },
        );

        let filter = nde::Filter {
            name: "foo".to_string(),
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
            events: EventTrack::from_vec(vec![event]),
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
        emit::emit(&mut emitted, &ass_file, Some(context)).unwrap();

        let (parsed, _warnings) = parse::tests::parse_str(&emitted);
        assert_eq!(parsed.events.len(), 24);
    }

    #[test]
    fn compile_comments() {
        // Test that comments are skipped in compilation

        let events = EventTrack::from_vec(vec![
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
        ]);

        let context = compile::Context {
            frame_rate: media::FrameRate {
                numerator: 24,
                denominator: 1,
            },
        };
        let compiled = events.compile(&Extradata::default(), &context, 0, None);

        assert_eq!(compiled.len(), 1);
    }
}
