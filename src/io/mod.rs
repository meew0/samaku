use std::collections::{BTreeMap, HashMap};

use crate::subtitle;

pub mod emit;
pub mod parse;

pub struct AssFile {
    subtitles: subtitle::SlineTrack,
    side_data: SideData,
}

pub struct SideData {
    script_info: subtitle::ScriptInfo,
    extradata: Extradata,
    other_sections: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct Extradata {
    entries: BTreeMap<u32, ExtradataEntry>,
    next_id: u32,
}

impl Extradata {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub struct ExtradataEntry {
    key: String,
    value: String,
}
