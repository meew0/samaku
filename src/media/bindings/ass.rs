#![allow(dead_code)]

use libass_sys as libass;

pub type CString = std::ffi::CString;

pub fn ms_to_frame(ass_ms: i64, fps: f64) -> i32 {
    let ass_seconds: f64 = ass_ms as f64 / 1000.0;
    (ass_seconds * fps) as i32
}

pub fn frame_to_ms(frame: i32, fps: f64) -> i64 {
    let ass_seconds: f64 = frame as f64 / fps;
    (ass_seconds * 1000.0) as i64
}

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
                codepage.map_or(0 as *mut i8, |cp| cp.as_ptr() as *mut i8),
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

pub enum FontProvider {
    None = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_NONE as isize,
    Autodetect = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_AUTODETECT as isize,
    CoreText = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_CORETEXT as isize,
    Fontconfig = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_FONTCONFIG as isize,
    DirectWrite = libass::ASS_DefaultFontProvider::ASS_FONTPROVIDER_DIRECTWRITE as isize,
}

pub enum RenderChange {
    Identical,
    DifferentPositions,
    DifferentContent,
}

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

    fn render_frame_internal<F: FnMut(&Image) -> ()>(
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

    pub fn render_frame_detect_change<F: FnMut(&Image) -> ()>(
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

    pub fn render_frame<F: FnMut(&Image) -> ()>(&self, track: &Track, now: i64, callback: &mut F) {
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

pub struct Track {
    track: *mut libass::ASS_Track,
}

impl Track {
    pub fn events_mut(&self) -> &mut [RawEvent] {
        unsafe {
            std::slice::from_raw_parts_mut((*self.track).events, (*self.track).n_events as usize)
        }
    }

    pub fn events(&self) -> &[RawEvent] {
        self.events_mut()
    }

    pub fn styles_mut(&self) -> &mut [RawStyle] {
        unsafe {
            std::slice::from_raw_parts_mut((*self.track).styles, (*self.track).n_styles as usize)
        }
    }

    pub fn styles(&self) -> &[RawStyle] {
        self.styles_mut()
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
