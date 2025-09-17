//! Utility types and methods for storing Samaku session data (like pane layouts or loaded videos) in ASS files
#![allow(
    clippy::min_ident_chars,
    reason = "iced's pane grid uses `a` and `b` consistently and it makes sense to use these as well here"
)]

use crate::{config, pane, subtitle};
use anyhow::Context as _;
use iced::widget::pane_grid;
use std::borrow::Cow;
use std::collections::HashSet;
use thiserror::Error;

/// Serialize data into Samaku's preferred alphanumeric binary format (czb = CBOR + zlib + base64)
///
/// # Errors
/// Returns an error when serialization failed, see `ciborium`'s error type for details
pub fn serialize_czb<T: ?Sized + serde::Serialize>(
    value: &T,
    compression_level: u8,
) -> anyhow::Result<String> {
    let mut data: Vec<u8> = vec![];
    ciborium::into_writer(value, &mut data)?;

    Ok(data_encoding::BASE64.encode(
        miniz_oxide::deflate::compress_to_vec(data.as_slice(), compression_level).as_slice(),
    ))
}

/// Deserialize data from Samaku's preferred alphanumeric binary format (czb = CBOR + zlib + base64)
///
/// # Errors
/// Returns an error when deserialization failed, see `DeserializeError` variants for details
pub fn deserialize_czb<T: serde::de::DeserializeOwned>(
    value: &[u8],
) -> Result<T, DeserializeError> {
    let decoded = data_encoding::BASE64
        .decode(value)
        .map_err(DeserializeError::Base64DecodeError)?;
    let decompressed =
        miniz_oxide::inflate::decompress_to_vec_with_limit(decoded.as_slice(), 1_000_000)
            .map_err(DeserializeError::DecompressError)?;
    ciborium::from_reader::<T, _>(decompressed.as_slice())
        .map_err(|de_error| DeserializeError::DeserialiseError(format!("{de_error:?}")))
}

#[derive(Error, Debug)]
pub enum DeserializeError {
    #[error("Failed to decode base64 data for NDE filter: {0}")]
    Base64DecodeError(data_encoding::DecodeError),

    #[error("Failed to decompress NDE filter: {0}")]
    DecompressError(miniz_oxide::inflate::DecompressError),

    #[error("Failed to deserialise NDE filter: {0}")]
    DeserialiseError(String),
}

pub const METADATA_KEY: &str = "Samaku Project Metadata";

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Project<'a> {
    pane_layout: PaneLayout<'a>,
    selected_event_indices: Cow<'a, HashSet<subtitle::EventIndex>>,
}

impl<'a> Project<'a> {
    #[must_use]
    pub fn compile_from(
        panes: &'a pane_grid::State<pane::State>,
        selected_event_indices: &'a HashSet<subtitle::EventIndex>,
    ) -> Self {
        let pane_layout = PaneLayout::from_pane_grid(panes, panes.layout());

        Self {
            pane_layout,
            selected_event_indices: Cow::Borrowed(selected_event_indices),
        }
    }

    pub fn update_global(self, global_state: &mut crate::Samaku) {
        let Self {
            pane_layout,
            selected_event_indices,
        } = self;
        global_state.panes = pane_grid::State::with_configuration(pane_layout.into_configuration());
        global_state.selected_event_indices = selected_event_indices.into_owned();
    }

    pub fn load(subtitle_file: &subtitle::File) -> anyhow::Result<Option<Self>> {
        if let Some(czb) = subtitle_file.script_info.extra_info.get(METADATA_KEY) {
            let project = deserialize_czb::<Project>(czb.as_bytes())
                .context("Failed to deserialize project metadata")?;
            Ok(Some(project))
        } else {
            println!("No project metadata found in opened subtitle file");
            Ok(None)
        }
    }

    pub fn store(&self, subtitle_file: &mut subtitle::File) -> anyhow::Result<()> {
        let czb = serialize_czb(&self, config::PROJECT_COMPRESSION_LEVEL)?;
        subtitle_file
            .script_info
            .extra_info
            .insert(METADATA_KEY.to_owned(), czb);
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
enum PaneLayout<'a> {
    Split {
        #[serde(with = "AxisDef")]
        axis: pane_grid::Axis,
        ratio: f32,
        a: Box<PaneLayout<'a>>,
        b: Box<PaneLayout<'a>>,
    },
    Pane(PaneCow<'a>),
}

impl<'a> PaneLayout<'a> {
    fn from_pane_grid(
        pane_grid_state: &'a pane_grid::State<pane::State>,
        node: &pane_grid::Node,
    ) -> Self {
        match node {
            pane_grid::Node::Split {
                axis, ratio, a, b, ..
            } => Self::Split {
                axis: *axis,
                ratio: *ratio,
                a: Box::new(PaneLayout::from_pane_grid(pane_grid_state, a)),
                b: Box::new(PaneLayout::from_pane_grid(pane_grid_state, b)),
            },
            pane_grid::Node::Pane(pane) => Self::Pane(PaneCow::Borrowed(
                pane_grid_state
                    .panes
                    .get(pane)
                    .expect("found invalid pane reference in pane grid"),
            )),
        }
    }

    fn into_configuration(self) -> pane_grid::Configuration<pane::State> {
        match self {
            PaneLayout::Split { axis, ratio, a, b } => pane_grid::Configuration::Split {
                axis,
                ratio,
                a: Box::new(a.into_configuration()),
                b: Box::new(b.into_configuration()),
            },
            PaneLayout::Pane(cow) => pane_grid::Configuration::Pane(match cow {
                PaneCow::Borrowed(_) => {
                    panic!("tried to convert borrowed PaneCow in to_configuration")
                }
                PaneCow::Owned(state) => state,
            }),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "iced::widget::pane_grid::Axis")]
enum AxisDef {
    Horizontal,
    Vertical,
}

enum PaneCow<'a> {
    Borrowed(&'a pane::State),
    Owned(pane::State),
}

impl serde::Serialize for PaneCow<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            PaneCow::Borrowed(state) => state.serialize(serializer),
            PaneCow::Owned(state) => state.serialize(serializer),
        }
    }
}

impl<'de> serde::Deserialize<'de> for PaneCow<'_> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(PaneCow::Owned(pane::State::deserialize(deserializer)?))
    }
}
