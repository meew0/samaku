#![allow(dead_code)]

use std::ffi::CStr;

use libass_sys as libass;

use crate::subtitle;

pub type CString = std::ffi::CString;

pub fn ms_to_frame(ass_ms: i64, fps: f64) -> i32 {
    let ass_seconds: f64 = ass_ms as f64 / 1000.0;
    (ass_seconds * fps) as i32
}

pub fn frame_to_ms(frame: i32, fps: f64) -> i64 {
    let ass_seconds: f64 = frame as f64 / fps;
    (ass_seconds * 1000.0) as i64
}

unsafe fn str_from_libass<'a>(ptr: *const i8) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }

    Some(
        unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .expect("text data returned from libass should be UTF-8"),
    )
}

fn string_from_libass(ptr: *const i8) -> Option<String> {
    unsafe { str_from_libass(ptr) }.map(|str| str.to_owned())
}

/// Allocate an empty string of required length with libc's malloc specifically,
/// as libass uses that as well, and requires to be able to free strings
/// that are passed into it.
fn malloc_string(source: &str) -> *mut i8 {
    let c_string =
        CString::new(source).expect("string passed to malloc_string should be free of null bytes");
    let source_slice = c_string.to_bytes_with_nul();
    let len = source_slice.len();

    let ptr = unsafe { libc::malloc(len) };
    if ptr.is_null() {
        panic!("malloc in malloc_string returned null pointer, out of memory?");
    }

    let target_slice: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, len) };
    target_slice.copy_from_slice(source_slice);

    ptr as *mut i8
}

#[derive(Debug)]
pub struct Library {
    library: *mut libass::ASS_Library,
}

unsafe impl Send for Library {}

unsafe impl Sync for Library {}

impl Library {
    pub fn init() -> Option<Library> {
        let library = unsafe { libass::ass_library_init() };
        if library.is_null() {
            None
        } else {
            Some(Library { library })
        }
    }

    pub fn renderer_init(&self) -> Option<Renderer> {
        let renderer = unsafe { libass::ass_renderer_init(self.library) };
        if renderer.is_null() {
            None
        } else {
            Some(Renderer { renderer })
        }
    }

    pub fn new_track(&self) -> Option<Track> {
        let track = unsafe { libass::ass_new_track(self.library) };
        if track.is_null() {
            None
        } else {
            Some(Track { track })
        }
    }

    pub fn read_memory(&self, buf: &[u8], codepage: Option<CString>) -> Option<Track> {
        let track = unsafe {
            libass::ass_read_memory(
                self.library,
                buf.as_ptr() as *mut i8,
                buf.len(),
                codepage.map_or(std::ptr::null_mut::<i8>(), |cp| cp.as_ptr() as *mut i8),
            )
        };
        if track.is_null() {
            None
        } else {
            Some(Track { track })
        }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        unsafe { libass::ass_library_done(self.library) };
    }
}

pub static LIBRARY: once_cell::sync::Lazy<Library> =
    once_cell::sync::Lazy::new(|| Library::init().unwrap());

#[derive(Debug, Clone, Copy)]
pub enum FontProvider {
    None = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_NONE as isize,
    Autodetect = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_AUTODETECT as isize,
    CoreText = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_CORETEXT as isize,
    Fontconfig = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_FONTCONFIG as isize,
    DirectWrite = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_DIRECTWRITE as isize,
}

#[derive(Debug, Clone, Copy)]
pub enum RenderChange {
    Identical,
    DifferentPositions,
    DifferentContent,
}

#[derive(Debug)]
pub struct Renderer {
    renderer: *mut libass::ASS_Renderer,
}

impl Renderer {
    pub fn set_frame_size(&mut self, w: i32, h: i32) {
        unsafe { libass::ass_set_frame_size(self.renderer, w, h) }
    }

    pub fn set_storage_size(&mut self, w: i32, h: i32) {
        unsafe { libass::ass_set_storage_size(self.renderer, w, h) }
    }

    pub fn set_fonts(
        &mut self,
        default_font: Option<CString>,
        default_family: CString,
        default_font_provider: FontProvider,
        fontconfig_config: Option<CString>,
        update: bool,
    ) {
        unsafe {
            libass::ass_set_fonts(
                self.renderer,
                default_font.map_or(std::ptr::null(), |s| s.as_ptr()),
                default_family.as_ptr(),
                default_font_provider as i32,
                fontconfig_config.map_or(std::ptr::null(), |s| s.as_ptr()),
                update as i32,
            )
        }
    }

    fn render_frame_internal<F: FnMut(&Image)>(
        &self,
        track: &Track,
        now: i64,
        detect_change: bool,
        callback: &mut F,
    ) -> i32 {
        let mut change = if detect_change { 1 } else { 0 };
        let mut image =
            unsafe { libass::ass_render_frame(self.renderer, track.track, now, &mut change) };

        // Call the callback for each returned image.
        // Rust has no elegant way to express the idea of
        // “this object lives until the next function invocation”,
        // as the lifetime for an ASS_Image would be,
        // so using a callback that just gets a reference to a wrapped image
        // ensures safety compared to passing “ownership” of it.
        while !image.is_null() {
            let bitmap_size = unsafe { (*image).stride * ((*image).h - 1) + (*image).w };
            let safe_image = Image {
                metadata: unsafe { &(*image) },
                bitmap: unsafe {
                    std::slice::from_raw_parts((*image).bitmap, bitmap_size as usize)
                },
            };
            callback(&safe_image);
            image = unsafe { (*image).next };
        }

        change
    }

    pub fn render_frame_detect_change<F: FnMut(&Image)>(
        &self,
        track: &Track,
        now: i64,
        callback: &mut F,
    ) -> RenderChange {
        match self.render_frame_internal(track, now, true, callback) {
            0 => RenderChange::Identical,
            1 => RenderChange::DifferentPositions,
            2 => RenderChange::DifferentContent,
            n => panic!("Invalid detect_change value: {}", n),
        }
    }

    pub fn render_frame<F: FnMut(&Image)>(&self, track: &Track, now: i64, callback: &mut F) {
        self.render_frame_internal(track, now, false, callback);
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe { libass::ass_renderer_done(self.renderer) }
    }
}

pub type RawEvent = libass::ASS_Event;
pub type RawStyle = libass::ASS_Style;

pub fn raw_event_to_sline(raw_event: &RawEvent) -> subtitle::Sline {
    subtitle::Sline {
        start: subtitle::StartTime(raw_event.Start),
        duration: subtitle::Duration(raw_event.Duration),
        style_index: raw_event.Style,
        layer_index: raw_event.Layer,
        margins: subtitle::Margins {
            left: raw_event.MarginL,
            right: raw_event.MarginR,
            vertical: raw_event.MarginV,
        },
        text: string_from_libass(raw_event.Text).expect("event text should never be null"),
    }
}

pub fn event_to_raw(event: &subtitle::ass::Event) -> RawEvent {
    RawEvent {
        Start: event.start.0,
        Duration: event.duration.0,
        ReadOrder: event.read_order,
        Layer: event.layer_index,
        Style: event.style_index,
        Name: malloc_string(event.name),
        MarginL: event.margins.left,
        MarginR: event.margins.right,
        MarginV: event.margins.vertical,
        Effect: malloc_string(event.effect),
        Text: malloc_string(event.text),
        render_priv: std::ptr::null_mut(),
    }
}

pub fn style_from_raw(raw_style: &RawStyle) -> subtitle::Style {
    subtitle::Style {
        name: string_from_libass(raw_style.Name).expect("style name should never be null"),
        font_name: string_from_libass(raw_style.FontName)
            .expect("style font name should never be null"),
        font_size: raw_style.FontSize,
        primary_colour: subtitle::Colour::unpack(raw_style.PrimaryColour),
        secondary_colour: subtitle::Colour::unpack(raw_style.SecondaryColour),
        outline_colour: subtitle::Colour::unpack(raw_style.OutlineColour),
        back_colour: subtitle::Colour::unpack(raw_style.BackColour),
        bold: raw_style.Bold != 0,
        italic: raw_style.Italic != 0,
        underline: raw_style.Underline != 0,
        strike_out: raw_style.StrikeOut != 0,
        scale: subtitle::Scale {
            x: raw_style.ScaleX,
            y: raw_style.ScaleY,
        },
        spacing: raw_style.Spacing,
        angle: subtitle::Angle(raw_style.Angle),
        border_style: subtitle::BorderStyle::from(raw_style.BorderStyle),
        outline: raw_style.Outline,
        shadow: raw_style.Shadow,
        alignment: subtitle::Alignment::try_unpack(raw_style.Alignment)
            .expect("received invalid alignment value from libass"),
        margins: subtitle::Margins {
            left: raw_style.MarginL,
            right: raw_style.MarginR,
            vertical: raw_style.MarginV,
        },
        encoding: raw_style.Encoding,
        blur: raw_style.Blur,
        justify: subtitle::JustifyMode::from(raw_style.Justify),
    }
}

pub fn style_to_raw(style: &subtitle::Style) -> RawStyle {
    RawStyle {
        Name: malloc_string(style.name.as_str()),
        FontName: malloc_string(style.font_name.as_str()),
        FontSize: style.font_size,
        PrimaryColour: style.primary_colour.pack(),
        SecondaryColour: style.secondary_colour.pack(),
        OutlineColour: style.outline_colour.pack(),
        BackColour: style.back_colour.pack(),
        Bold: style.bold as i32,
        Italic: style.italic as i32,
        Underline: style.underline as i32,
        StrikeOut: style.strike_out as i32,
        ScaleX: style.scale.x,
        ScaleY: style.scale.y,
        Spacing: style.spacing,
        Angle: style.angle.0,
        BorderStyle: style.border_style as i32,
        Outline: style.outline,
        Shadow: style.shadow,
        Alignment: style.alignment.pack(),
        MarginL: style.margins.left,
        MarginR: style.margins.right,
        MarginV: style.margins.vertical,
        Encoding: style.encoding,
        treat_fontname_as_pattern: 0, // unused within libass
        Blur: style.blur,
        Justify: style.justify as i32,
    }
}

#[derive(Debug)]
pub struct Track {
    track: *mut libass::ASS_Track,
}

impl Track {
    pub fn events_mut(&mut self) -> &mut [RawEvent] {
        unsafe {
            std::slice::from_raw_parts_mut((*self.track).events, (*self.track).n_events as usize)
        }
    }

    pub fn events(&self) -> &[RawEvent] {
        unsafe { std::slice::from_raw_parts((*self.track).events, (*self.track).n_events as usize) }
    }

    pub fn styles_mut(&mut self) -> &mut [RawStyle] {
        unsafe {
            std::slice::from_raw_parts_mut((*self.track).styles, (*self.track).n_styles as usize)
        }
    }

    pub fn styles(&self) -> &[RawStyle] {
        unsafe { std::slice::from_raw_parts((*self.track).styles, (*self.track).n_styles as usize) }
    }

    pub fn alloc_event(&mut self) {
        unsafe {
            libass::ass_alloc_event(self.track);
        }
    }

    pub fn alloc_style(&mut self) {
        unsafe {
            libass::ass_alloc_style(self.track);
        }
    }

    pub fn header(&self) -> subtitle::ass::TrackHeader {
        subtitle::ass::TrackHeader {
            play_res: subtitle::Resolution {
                x: unsafe { (*self.track).PlayResX },
                y: unsafe { (*self.track).PlayResY },
            },
            timer: unsafe { (*self.track).Timer },
            wrap_style: subtitle::WrapStyle::from(unsafe { (*self.track).WrapStyle }),
            scaled_border_and_shadow: unsafe { (*self.track).ScaledBorderAndShadow } != 0,
            kerning: unsafe { (*self.track).Kerning } != 0,
            language: unsafe { str_from_libass((*self.track).Language) },
            ycbcr_matrix: match unsafe { (*self.track).YCbCrMatrix } {
                libass::ASS_YCbCrMatrix::YCBCR_DEFAULT => subtitle::ass::YCbCrMatrix::Default,
                libass::ASS_YCbCrMatrix::YCBCR_UNKNOWN => subtitle::ass::YCbCrMatrix::Unknown,
                libass::ASS_YCbCrMatrix::YCBCR_NONE => subtitle::ass::YCbCrMatrix::None,
                libass::ASS_YCbCrMatrix::YCBCR_BT601_TV => subtitle::ass::YCbCrMatrix::Bt601Tv,
                libass::ASS_YCbCrMatrix::YCBCR_BT601_PC => subtitle::ass::YCbCrMatrix::Bt601Pc,
                libass::ASS_YCbCrMatrix::YCBCR_BT709_TV => subtitle::ass::YCbCrMatrix::Bt709Tv,
                libass::ASS_YCbCrMatrix::YCBCR_BT709_PC => subtitle::ass::YCbCrMatrix::Bt709Pc,
                libass::ASS_YCbCrMatrix::YCBCR_SMPTE240M_TV => {
                    subtitle::ass::YCbCrMatrix::Smtpe240MPc
                }
                libass::ASS_YCbCrMatrix::YCBCR_SMPTE240M_PC => {
                    subtitle::ass::YCbCrMatrix::Smtpe240MTv
                }
                libass::ASS_YCbCrMatrix::YCBCR_FCC_TV => subtitle::ass::YCbCrMatrix::FccTv,
                libass::ASS_YCbCrMatrix::YCBCR_FCC_PC => subtitle::ass::YCbCrMatrix::FccPc,

                // Honestly, it's debatable if we should even support tracks
                // that use a matrix other than `NONE`.
                _ => subtitle::ass::YCbCrMatrix::None,
            },
            name: unsafe { str_from_libass((*self.track).name) },
        }
    }

    pub fn set_header(&mut self, header: &subtitle::ass::TrackHeader) {
        unsafe {
            (*self.track).PlayResX = header.play_res.x;
            (*self.track).PlayResY = header.play_res.y;
            (*self.track).Timer = header.timer;
            (*self.track).WrapStyle = header.wrap_style as i32;
            (*self.track).ScaledBorderAndShadow = header.scaled_border_and_shadow as i32;
            (*self.track).Kerning = header.kerning as i32;
            (*self.track).YCbCrMatrix = header.ycbcr_matrix as u32;

            (*self.track).Language = match header.language {
                Some(language) => malloc_string(language),
                None => std::ptr::null_mut(),
            };
            (*self.track).name = match header.name {
                Some(name) => malloc_string(name),
                None => std::ptr::null_mut(),
            };
        }
    }
}

impl Drop for Track {
    fn drop(&mut self) {
        unsafe { libass::ass_free_track(self.track) };
    }
}

pub struct ImageType {}

pub type ImageInternal = libass::ASS_Image;

pub struct Image<'a> {
    pub metadata: &'a ImageInternal,
    pub bitmap: &'a [u8],
}
