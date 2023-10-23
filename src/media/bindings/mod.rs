use std::{ffi::CString, path::Path};

pub mod ass;
pub mod bestsource;
pub mod mv;
pub mod vapoursynth;

pub fn c_string<T: Into<Vec<u8>>>(rust_str: T) -> CString {
    CString::new(rust_str).expect("string passed into c_string should not contain null bytes")
}

pub fn path_to_cstring<P: AsRef<Path>>(path_as_ref: P) -> CString {
    // https://stackoverflow.com/a/59224987
    // Why is this not in the standard library?

    let path = path_as_ref.as_ref();
    let mut buf = Vec::new();

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        buf.extend(path.as_os_str().as_bytes());
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        buf.extend(
            path.as_os_str()
                .encode_wide()
                .map(|b| {
                    let b = b.to_ne_bytes();
                    b.get(0).map(|s| *s).into_iter().chain(b.get(1).map(|s| *s))
                })
                .flatten(),
        );
    }

    CString::new(buf).expect("path buffer should not contain null bytes")
}
