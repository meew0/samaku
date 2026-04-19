use std::{ffi::CString, path::Path};

pub(super) mod ass;
pub(super) mod ffms2;
pub(super) mod mv;

pub(super) fn c_string<T: Into<Vec<u8>>>(rust_str: T) -> CString {
    CString::new(rust_str).expect("string passed into c_string should not contain null bytes")
}

/// Convert a Rust `Path` to a C string for use across FFI boundaries.
/// Tries to do this as losslessly as possible, but ultimately, paths that are not valid Unicode
/// are not always supported.
pub(super) fn path_to_cstring<P: AsRef<Path>>(path_as_ref: P) -> CString {
    let path = path_as_ref.as_ref();
    let mut buf = Vec::new();

    #[cfg(unix)]
    {
        // On Unix, we can directly type-convert the byte sequence,
        // without having to make any assumptions that it is valid UTF-8
        // or anything else.
        use std::os::unix::ffi::OsStrExt as _;
        buf.extend(path.as_os_str().as_bytes());
    }

    #[cfg(windows)]
    {
        // ffms2/ffmpeg do not let us pass wide paths directly.
        // Internally, libavutil converts byte sequences to wide paths,
        // assuming they are valid UTF-8 (`win32_open` in `libavutil/file_open.c`).
        // So we have no choice but to only accept valid Unicode paths.
        buf.extend(
            path.as_os_str()
                .to_str()
                .expect("only valid Unicode paths are accepted on Windows")
                .as_bytes(),
        );
    }

    CString::new(buf).expect("path buffer should not contain null bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_paths() -> anyhow::Result<()> {
        super::super::init();
        let paths = &[
            crate::test_utils::test_file("test_files/unicode_paths/蘇.png"),
            crate::test_utils::test_file("test_files/unicode_paths/🌌.png"),
        ];

        for path in paths {
            let mut indexer = ffms2::Indexer::new(path)?;
            indexer.set_track_type_index_settings(ffms2::TrackType::Video, 1);
            indexer.do_indexing(ffms2::IndexErrorHandling::Abort)?;
        }

        Ok(())
    }
}
