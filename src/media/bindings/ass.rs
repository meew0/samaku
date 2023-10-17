#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::CStr;

use libass_sys as libass;

use crate::nde::tags::{Alignment, WrapStyle};
use crate::subtitle;
use crate::subtitle::{EventType, Margins, YCbCrMatrix};

pub type CString = std::ffi::CString;

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
    unsafe { str_from_libass(ptr) }.map(str::to_owned)
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
    assert!(
        !ptr.is_null(),
        "malloc in malloc_string returned null pointer, out of memory?"
    );

    let target_slice: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(ptr.cast::<u8>(), len) };
    target_slice.copy_from_slice(source_slice);

    ptr.cast::<i8>()
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
                codepage.map_or(std::ptr::null_mut::<i8>(), |cp| cp.as_ptr().cast_mut()),
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
#[repr(u32)]
pub enum FontProvider {
    None = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_NONE,
    Autodetect = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_AUTODETECT,
    CoreText = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_CORETEXT,
    Fontconfig = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_FONTCONFIG,
    DirectWrite = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_DIRECTWRITE,
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
        default_family: &CString,
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
                i32::from(update),
            );
        }
    }

    fn render_frame_internal<F: FnMut(&Image)>(
        &self,
        track: &Track,
        now: i64,
        detect_change: bool,
        callback: &mut F,
    ) -> i32 {
        let mut change = i32::from(detect_change);
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
                    #[allow(clippy::cast_sign_loss)]
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
            n => panic!("Invalid detect_change value: {n}"),
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
        actor: string_from_libass(raw_event.Name).unwrap_or_default(),
        effect: string_from_libass(raw_event.Effect).unwrap_or_default(),
        event_type: EventType::Dialogue,
        extradata_ids: vec![],
    }
}

pub fn event_to_raw(event: &subtitle::CompiledEvent) -> RawEvent {
    RawEvent {
        Start: event.start.0,
        Duration: event.duration.0,
        ReadOrder: event.read_order,
        Layer: event.layer_index,
        Style: event.style_index,
        Name: malloc_string(event.name.as_ref()),
        MarginL: event.margins.left,
        MarginR: event.margins.right,
        MarginV: event.margins.vertical,
        Effect: malloc_string(event.effect.as_ref()),
        Text: malloc_string(event.text.as_ref()),
        render_priv: std::ptr::null_mut(),
    }
}

pub fn style_from_raw(raw_style: &RawStyle) -> subtitle::Style {
    let (primary_colour, primary_transparency) =
        subtitle::unpack_colour_and_transparency_rgbt(raw_style.PrimaryColour);
    let (secondary_colour, secondary_transparency) =
        subtitle::unpack_colour_and_transparency_rgbt(raw_style.SecondaryColour);
    let (border_colour, border_transparency) =
        subtitle::unpack_colour_and_transparency_rgbt(raw_style.OutlineColour);
    let (shadow_colour, shadow_transparency) =
        subtitle::unpack_colour_and_transparency_rgbt(raw_style.BackColour);

    subtitle::Style {
        name: string_from_libass(raw_style.Name).expect("style name should never be null"),
        font_name: string_from_libass(raw_style.FontName)
            .expect("style font name should never be null"),
        font_size: raw_style.FontSize,
        primary_colour,
        secondary_colour,
        border_colour,
        shadow_colour,
        primary_transparency,
        secondary_transparency,
        border_transparency,
        shadow_transparency,
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
        border_width: raw_style.Outline,
        shadow_distance: raw_style.Shadow,
        alignment: Alignment::try_unpack(raw_style.Alignment)
            .expect("received invalid alignment value from libass"),
        margins: Margins {
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
        PrimaryColour: subtitle::pack_colour_and_transparency_rgbt(
            style.primary_colour,
            style.primary_transparency,
        ),
        SecondaryColour: subtitle::pack_colour_and_transparency_rgbt(
            style.secondary_colour,
            style.secondary_transparency,
        ),
        OutlineColour: subtitle::pack_colour_and_transparency_rgbt(
            style.border_colour,
            style.border_transparency,
        ),
        BackColour: subtitle::pack_colour_and_transparency_rgbt(
            style.shadow_colour,
            style.shadow_transparency,
        ),
        Bold: i32::from(style.bold),
        Italic: i32::from(style.italic),
        Underline: i32::from(style.underline),
        StrikeOut: i32::from(style.strike_out),
        ScaleX: style.scale.x,
        ScaleY: style.scale.y,
        Spacing: style.spacing,
        Angle: style.angle.0,
        BorderStyle: style.border_style as i32,
        Outline: style.border_width,
        Shadow: style.shadow_distance,
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
            #[allow(clippy::cast_sign_loss)]
            std::slice::from_raw_parts_mut((*self.track).events, (*self.track).n_events as usize)
        }
    }

    pub fn events(&self) -> &[RawEvent] {
        unsafe {
            #[allow(clippy::cast_sign_loss)]
            std::slice::from_raw_parts((*self.track).events, (*self.track).n_events as usize)
        }
    }

    pub fn styles_mut(&mut self) -> &mut [RawStyle] {
        unsafe {
            #[allow(clippy::cast_sign_loss)]
            std::slice::from_raw_parts_mut((*self.track).styles, (*self.track).n_styles as usize)
        }
    }

    pub fn styles(&self) -> &[RawStyle] {
        unsafe {
            #[allow(clippy::cast_sign_loss)]
            std::slice::from_raw_parts((*self.track).styles, (*self.track).n_styles as usize)
        }
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

    pub fn header(&self) -> subtitle::ScriptInfo {
        let mut extra_info: HashMap<String, String> = HashMap::new();

        if let Some(language) = unsafe { string_from_libass((*self.track).Language) } {
            extra_info.insert("Language".to_string(), language);
        }

        if let Some(title) = unsafe { string_from_libass((*self.track).name) } {
            extra_info.insert("Title".to_string(), title);
        }

        subtitle::ScriptInfo {
            extra_info,
            playback_resolution: subtitle::Resolution {
                x: unsafe { (*self.track).PlayResX },
                y: unsafe { (*self.track).PlayResY },
            },
            timer: unsafe { (*self.track).Timer },
            wrap_style: WrapStyle::from(unsafe { (*self.track).WrapStyle }),
            scaled_border_and_shadow: unsafe { (*self.track).ScaledBorderAndShadow } != 0,
            kerning: unsafe { (*self.track).Kerning } != 0,
            ycbcr_matrix: match unsafe { (*self.track).YCbCrMatrix } {
                libass::ASS_YCbCrMatrix::YCBCR_DEFAULT => YCbCrMatrix::Default,
                libass::ASS_YCbCrMatrix::YCBCR_UNKNOWN => YCbCrMatrix::Unknown,
                // implied by `_` arm
                // libass::ASS_YCbCrMatrix::YCBCR_NONE => YCbCrMatrix::None,
                libass::ASS_YCbCrMatrix::YCBCR_BT601_TV => YCbCrMatrix::Bt601Tv,
                libass::ASS_YCbCrMatrix::YCBCR_BT601_PC => YCbCrMatrix::Bt601Pc,
                libass::ASS_YCbCrMatrix::YCBCR_BT709_TV => YCbCrMatrix::Bt709Tv,
                libass::ASS_YCbCrMatrix::YCBCR_BT709_PC => YCbCrMatrix::Bt709Pc,
                libass::ASS_YCbCrMatrix::YCBCR_SMPTE240M_TV => YCbCrMatrix::Smtpe240MPc,
                libass::ASS_YCbCrMatrix::YCBCR_SMPTE240M_PC => YCbCrMatrix::Smtpe240MTv,
                libass::ASS_YCbCrMatrix::YCBCR_FCC_TV => YCbCrMatrix::FccTv,
                libass::ASS_YCbCrMatrix::YCBCR_FCC_PC => YCbCrMatrix::FccPc,

                // Honestly, it's debatable if we should even support tracks
                // that use a matrix other than `NONE`.
                _ => YCbCrMatrix::None,
            },
        }
    }

    pub fn set_header(&mut self, header: &subtitle::ScriptInfo) {
        unsafe {
            (*self.track).PlayResX = header.playback_resolution.x;
            (*self.track).PlayResY = header.playback_resolution.y;
            (*self.track).Timer = header.timer;
            (*self.track).WrapStyle = header.wrap_style as i32;
            (*self.track).ScaledBorderAndShadow = i32::from(header.scaled_border_and_shadow);
            (*self.track).Kerning = i32::from(header.kerning);
            (*self.track).YCbCrMatrix = header.ycbcr_matrix as u32;

            (*self.track).Language = match header.extra_info.get("Language") {
                Some(language) => malloc_string(language),
                None => std::ptr::null_mut(),
            };
            (*self.track).name = match header.extra_info.get("Title") {
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
