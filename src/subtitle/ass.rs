use std::borrow::Cow;

/// An event in true ASS terms, that is, one subtitle line
/// as it would be found in e.g. Aegisub. Not to be used
/// as the source for anything; only as an intermediate
/// in the conversion to events as used by libass directly
/// (`ASS_Event`)
///
/// See [`Sline`] docs for other fields.
#[derive(Debug, Clone)]
pub struct Event<'a> {
    pub start: super::StartTime,
    pub duration: super::Duration,
    pub layer_index: i32,
    pub style_index: i32,
    pub margins: super::Margins,
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
