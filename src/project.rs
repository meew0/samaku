//! Utility types and methods for storing Samaku session data (like pane layouts or loaded videos) in ASS files.
//!
//! The idea here is to enable collaboration between different ASS editors (Aegisub, Ameko) and samaku,
//! since samaku's focus is entirely on visual typesetting; for a full fansub project, other editors will be necessary
//! for timing etc.
//! So for the data we can save, we are basically limited by what other editors will resave upon editing.
//! For now, we only focus on what Aegisub does. In particular, Aegisub does not resave unknown sections,
//! so we cannot arbitrarily make those up for our own data, we need to be a bit more creative.

#![allow(
    clippy::min_ident_chars,
    reason = "iced's pane grid uses `a` and `b` consistently and it makes sense to use these as well here"
)]

use crate::subtitle::EventIndex;
use crate::{action, config, history, media, message, model, pane, subtitle, version};
use anyhow::Context as _;
use iced::advanced::graphics::futures::MaybeSend;
use iced::widget::pane_grid;
use std::borrow::Cow;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use thiserror::Error;

#[derive(Default)]
pub struct Project {
    pub save_path: Option<PathBuf>,
    pub saved_node: Option<Rc<RefCell<history::Node>>>,
    pub properties: Properties,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Properties {
    pub video_path: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
}

/// Serialize data into Samaku's preferred alphanumeric binary format (czb = CBOR + zlib + base64).
///
/// # Errors
/// Returns an error when serialization failed, see `ciborium`'s error type for details.
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

/// Deserialize data from Samaku's preferred alphanumeric binary format (czb = CBOR + zlib + base64).
///
/// # Errors
/// Returns an error when deserialization failed, see `DeserializeError` variants for details.
pub fn deserialize_czb<T: serde::de::DeserializeOwned>(
    value: &[u8],
) -> Result<T, DeserializeError> {
    let decoded = data_encoding::BASE64
        .decode(value)
        .map_err(DeserializeError::Base64Decode)?;
    let decompressed =
        miniz_oxide::inflate::decompress_to_vec_with_limit(decoded.as_slice(), 1_000_000)
            .map_err(DeserializeError::Decompress)?;
    ciborium::from_reader::<T, _>(decompressed.as_slice())
        .map_err(|de_error| DeserializeError::Deserialise(format!("{de_error:?}")))
}

#[derive(Error, Debug)]
pub enum DeserializeError {
    #[error("Failed to decode base64 data for NDE filter: {0}")]
    Base64Decode(data_encoding::DecodeError),

    #[error("Failed to decompress NDE filter: {0}")]
    Decompress(miniz_oxide::inflate::DecompressError),

    #[error("Failed to deserialise NDE filter: {0}")]
    Deserialise(String),
}

pub const METADATA_KEY: &str = "Samaku Project Metadata";

#[derive(serde::Serialize, serde::Deserialize)]
struct Store<'a> {
    pane_layout: PaneLayout<'a>,
    selected_events: Cow<'a, model::select::Selection<EventIndex>>,
    properties: Cow<'a, Properties>,
    motion_tracks: Cow<'a, media::motion::TrackList>,
    selected_tracks: Cow<'a, model::select::Selection<media::motion::TrackId>>,
}

#[derive(Debug, Clone, Copy)]
pub enum SaveMode {
    Over,
    OverAndClose(iced::window::Id),
    SaveAs,
}

impl SaveMode {
    #[must_use]
    pub fn close_after(self) -> Option<iced::window::Id> {
        if let SaveMode::OverAndClose(id) = self {
            Some(id)
        } else {
            None
        }
    }
}

pub fn save(global_state: &mut crate::Samaku, save_mode: SaveMode) -> iced::Task<message::Message> {
    let result = (|| {
        store(global_state).context("Failed to serialize project data")?;

        let mut data = String::new();
        subtitle::emit(&mut data, &global_state.subtitles, None)
            .context("subtitle::emit() failed")?; // should never happen

        Ok(data)
    })();

    if let Some(data) = global_state.toasts.anyhow(result) {
        if matches!(save_mode, SaveMode::Over | SaveMode::OverAndClose(_))
            && let Some(save_path) = global_state.project.save_path.as_ref()
        {
            let save_path_cloned = save_path.clone();

            // Save over
            let future = async {
                smol::fs::write(&save_path_cloned, data).await?;
                Ok(Some(save_path_cloned))
            };
            perform_save_future(save_mode, future)
        } else {
            // Save as: select a file path and save the data there
            let future = async {
                select_file_and_save(data)
                    .await
                    .context("Failed to write to file")
            };
            perform_save_future(save_mode, future)
        }
    } else {
        iced::Task::none()
    }
}

fn perform_save_future<
    F: Future<Output = anyhow::Result<Option<PathBuf>>> + MaybeSend + 'static,
>(
    save_mode: SaveMode,
    future: F,
) -> iced::Task<message::Message> {
    iced::Task::perform(
        future,
        message::Message::map_anyhow(move |path_opt| {
            message::Message::AfterSave(save_mode, path_opt)
        }),
    )
}

/// Copy project data from subtitle data into global state.
/// On success, returns a boolean whether project metadata was found or not.
pub fn load(global_state: &mut crate::Samaku) -> anyhow::Result<bool> {
    if let Some(czb) = global_state
        .subtitles
        .script_info
        .extra_info
        .get(METADATA_KEY)
    {
        let project = deserialize_czb::<Store>(czb.as_bytes())
            .context("Failed to deserialize project metadata")?;
        let Store {
            pane_layout,
            selected_events,
            properties,
            motion_tracks,
            selected_tracks,
        } = project;
        global_state.panes = pane_grid::State::with_configuration(pane_layout.into_configuration());
        global_state.selected_events = selected_events.into_owned();
        global_state.project.properties = properties.into_owned();
        global_state.motion_tracks = motion_tracks.into_owned();
        global_state.selected_tracks = selected_tracks.into_owned();
        Ok(true)
    } else {
        println!("No project metadata found in opened subtitle file");
        Ok(false)
    }
}

/// Copy project data from the global state into subtitle data.
pub fn store(global_state: &mut crate::Samaku) -> anyhow::Result<()> {
    let pane_layout = PaneLayout::from_pane_grid(&global_state.panes, global_state.panes.layout());

    let project = Store {
        pane_layout,
        selected_events: Cow::Borrowed(&global_state.selected_events),
        // TODO store video/audio paths to be relative to the subtitle/project file (e.g. using the `pathdiff` crate)
        properties: Cow::Borrowed(&global_state.project.properties),
        motion_tracks: Cow::Borrowed(&global_state.motion_tracks),
        selected_tracks: Cow::Borrowed(&global_state.selected_tracks),
    };

    let czb = serialize_czb(&project, config::PROJECT_COMPRESSION_LEVEL)?;
    global_state
        .subtitles
        .script_info
        .extra_info
        .insert(METADATA_KEY.to_owned(), czb);
    Ok(())
}

/// Perform after-load tasks such as opening linked audio and video files.
pub fn after_load(global_state: &mut crate::Samaku) -> iced::Task<message::Message> {
    if let Some(ref video_path) = global_state.project.properties.video_path {
        action::index_video_and_load(global_state, video_path.clone());
    }

    if let Some(ref audio_path) = global_state.project.properties.audio_path {
        action::load_audio(global_state, audio_path.clone());
    }

    iced::Task::none()
}

pub fn window_title(global_state: &crate::Samaku) -> String {
    let is_dirty = global_state.history.is_dirty(&global_state.project);
    if let Some(save_path) = global_state.project.save_path.as_ref()
        && let Some(file_name) = save_path.file_name()
    {
        let star = if is_dirty { "*" } else { "" };
        format!("{}{star} — samaku {}", file_name.display(), version::Long)
    } else if is_dirty {
        format!("(unsaved changes) — samaku {}", version::Long)
    } else {
        format!("samaku {}", version::Long)
    }
}

pub async fn select_file_and_save(data: String) -> anyhow::Result<Option<PathBuf>> {
    if let Some(handle) = rfd::AsyncFileDialog::new().save_file().await {
        smol::fs::write(handle.path(), data).await?;
        return Ok(Some(handle.path().to_owned()));
    }

    // No file selected
    Ok(None)
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
        match *node {
            pane_grid::Node::Split {
                axis,
                ratio,
                ref a,
                ref b,
                ..
            } => Self::Split {
                axis,
                ratio,
                a: Box::new(PaneLayout::from_pane_grid(pane_grid_state, a)),
                b: Box::new(PaneLayout::from_pane_grid(pane_grid_state, b)),
            },
            pane_grid::Node::Pane(ref pane) => Self::Pane(PaneCow::Borrowed(
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
                    panic!("tried to convert borrowed PaneCow into Configuration")
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
        #[expect(
            clippy::match_same_arms,
            reason = "cannot merge these arms here since state is bound inconsistently"
        )]
        match *self {
            PaneCow::Borrowed(state) => state.serialize(serializer),
            PaneCow::Owned(ref state) => state.serialize(serializer),
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
