pub mod ass;
pub mod vapoursynth;

pub fn c_string<T: Into<Vec<u8>>>(rust_str: T) -> std::ffi::CString {
    std::ffi::CString::new(rust_str).unwrap()
}
