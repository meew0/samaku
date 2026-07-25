use anyhow::Context as _;
use std::ffi::OsStr;
use std::path::Path;

mod matroska;

/// The result of an import.
#[derive(Debug, Clone, Default)]
pub struct Contents {
    pub text: String,

    /// Files attached to the source container (for example, the fonts needed to render the subtitles).
    pub attachments: Vec<Attachment>,
}

/// A file attached to a subtitle container.
#[derive(Debug, Clone)]
pub struct Attachment {
    pub name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

impl Attachment {
    /// Heuristics to determine whether this attachment looks like a font that libass could make use of.
    ///
    /// Muxers are inconsistent about attachment MIME types. The same TTF may be labelled
    /// `font/ttf`, `application/x-truetype-font`, `application/font-sfnt`, or even
    /// `application/octet-stream`, so the file extension is checked as well.
    #[must_use]
    pub fn is_font(&self) -> bool {
        const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "otc", "pfb"];

        if self.mime_type.contains("font") {
            return true;
        }

        Path::new(&self.name)
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| {
                FONT_EXTENSIONS
                    .iter()
                    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
            })
    }
}

pub async fn import(path: &Path) -> anyhow::Result<Contents> {
    match path.extension().and_then(OsStr::to_str) {
        Some("mkv" | "mka" | "mks") => {
            // If we find a matroska file, parse it and read the subtitles from there.
            matroska::open_and_read(path).await
        }
        _ => {
            // Otherwise just read the file normally (assuming it is an .ass file)
            // TODO verify this and add further subtitle formats for importing
            read_plain(path).await
        }
    }
}

async fn read_plain(path: &Path) -> anyhow::Result<Contents> {
    let content = smol::fs::read_to_string(path)
        .await
        .context("Failed to open file")?;
    Ok(Contents {
        text: content,
        attachments: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(name: &str, mime_type: &str) -> Attachment {
        Attachment {
            name: name.to_owned(),
            mime_type: mime_type.to_owned(),
            data: vec![],
        }
    }

    #[test]
    fn font_detection() {
        assert!(attachment("Barlow-Regular.ttf", "font/ttf").is_font());
        assert!(attachment("x.otf", "application/vnd.ms-opentype").is_font());
        // Muxers that give up and use a generic MIME type still have to be recognised.
        assert!(attachment("x.TTC", "application/octet-stream").is_font());
        assert!(attachment("no-extension", "application/x-truetype-font").is_font());

        assert!(!attachment("cover.jpg", "image/jpeg").is_font());
        assert!(!attachment("notes.txt", "text/plain").is_font());
        assert!(!attachment("no-extension", "application/octet-stream").is_font());
    }
}
