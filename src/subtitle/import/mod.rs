use anyhow::Context as _;
use std::ffi::OsStr;
use std::path::Path;

mod matroska;

pub async fn import(path: &Path) -> anyhow::Result<String> {
    match path.extension().and_then(OsStr::to_str) {
        Some("mkv" | "mka" | "mks") => {
            // If we find a matroska file, parse it and read the subtitles from there.
            // TODO make this async
            matroska::open_and_read(path)
        }
        _ => {
            // Otherwise just read the file normally (assuming it is an .ass file)
            // TODO verify this and add further subtitle formats for importing
            read_plain(path).await
        }
    }
}

async fn read_plain(path: &Path) -> anyhow::Result<String> {
    let content = smol::fs::read_to_string(path)
        .await
        .context("Failed to open file")?;
    Ok(content)
}
