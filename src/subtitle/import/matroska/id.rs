//! Element IDs, as listed in the Matroska specification,
//! including the marker bits of their variable-size encoding,
//! so they can be compared against decoded IDs directly.

#![expect(
    clippy::unreadable_literal,
    reason = "to match the way the IDs are written in the spec"
)]

use super::ebml::Id;

pub(super) const EBML: Id = Id(0x1A45DFA3);
pub(super) const DOC_TYPE: Id = Id(0x4282);
pub(super) const SEGMENT: Id = Id(0x18538067);

pub(super) const INFO: Id = Id(0x1549A966);
pub(super) const TIMESTAMP_SCALE: Id = Id(0x2AD7B1);

pub(super) const TRACKS: Id = Id(0x1654AE6B);
pub(super) const TRACK_ENTRY: Id = Id(0xAE);
pub(super) const TRACK_NUMBER: Id = Id(0xD7);
pub(super) const TRACK_TYPE: Id = Id(0x83);
pub(super) const CODEC_ID: Id = Id(0x86);
pub(super) const CODEC_PRIVATE: Id = Id(0x63A2);

pub(super) const CONTENT_ENCODINGS: Id = Id(0x6D80);
pub(super) const CONTENT_ENCODING: Id = Id(0x6240);
pub(super) const CONTENT_ENCODING_SCOPE: Id = Id(0x5032);
pub(super) const CONTENT_ENCODING_TYPE: Id = Id(0x5033);
pub(super) const CONTENT_COMPRESSION: Id = Id(0x5034);
pub(super) const CONTENT_COMP_ALGO: Id = Id(0x4254);

pub(super) const CLUSTER: Id = Id(0x1F43B675);
pub(super) const TIMESTAMP: Id = Id(0xE7);
pub(super) const BLOCK_GROUP: Id = Id(0xA0);
pub(super) const BLOCK: Id = Id(0xA1);
pub(super) const BLOCK_DURATION: Id = Id(0x9B);
pub(super) const SIMPLE_BLOCK: Id = Id(0xA3);

pub(super) const ATTACHMENTS: Id = Id(0x1941A469);
pub(super) const ATTACHED_FILE: Id = Id(0x61A7);
pub(super) const FILE_NAME: Id = Id(0x466E);
pub(super) const FILE_MIME_TYPE: Id = Id(0x4660);
pub(super) const FILE_DATA: Id = Id(0x465C);
