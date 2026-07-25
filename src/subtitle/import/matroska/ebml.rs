//! Minimal EBML primitives, sufficient to read the parts of a Matroska file that we care about.
//!
//! Elements are traversed in one of two ways, depending on how large they are expected to be:
//!
//!  - Large or open-ended elements (the segment itself, clusters) are traversed by a [`Reader`],
//!    which pulls one element header at a time out of an async source and can step over element
//!    bodies without ever copying them into memory. This is what lets us walk past video and audio
//!    payloads essentially for free.
//!  - Small elements (a track entry, an attached file, a block group) are read into memory in one
//!    go and then traversed by a [`Cursor`], which is far more convenient because it can hand out
//!    borrowed sub-slices for the element bodies.
//!
//! Both share the same variable-size integer decoders, which are pure functions over byte slices.

use smol::io::{AsyncReadExt as _, AsyncSeekExt as _};
use thiserror::Error;

/// The maximum number of bytes an element ID may occupy (the default `EBMLMaxIDLength`).
const MAX_ID_LENGTH: usize = 4;

/// The maximum number of bytes an element size may occupy (the default `EBMLMaxSizeLength`).
const MAX_SIZE_LENGTH: usize = 8;

/// Beyond this many bytes, [`Reader::skip`] seeks instead of reading and discarding. Small skips
/// are better served by reading, since seeking always discards the internal buffer.
const SEEK_THRESHOLD: u64 = 8192;

/// An upper bound on the size of an element that may be read into memory in one go.
///
/// This exists purely so that a corrupt or malicious length field cannot make us attempt a
/// multi-terabyte allocation. It is far above any legitimate track entry, attached file, or block
/// group; fonts, the largest thing we expect to buffer, run to a few megabytes at most.
const MAX_BUFFERED_ELEMENT_SIZE: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Id(pub u32);

impl Id {
    /// Decodes an element ID from a slice of bytes.
    /// Like decoding a `vint`, but retains the marker bit,
    /// so it can be compared to the ID constants which also contain this marker bit.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
        debug_assert!(
            bytes.len() <= MAX_ID_LENGTH,
            "element ID should not exceed {MAX_ID_LENGTH} bytes"
        );

        let mut value: u32 = 0;
        for &byte in bytes {
            value = (value << 8_i32) | u32::from(byte);
        }
        Id(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Size {
    /// Known size in bytes (usual case).
    Known(u64),

    /// Unknown size, e.g. streamed file.
    Unknown,
}

impl Size {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
        match decode_vint_value(bytes) {
            Some(value) => Self::Known(value),
            None => Self::Unknown,
        }
    }
}

/// The number of bytes a variable-size integer will occupy, including the byte it starts with,
/// according to the first byte.
///
/// Returns `None` for a zero byte, which is invalid according to the Matroska spec.
fn vint_length(first_byte: u8) -> Option<usize> {
    if first_byte == 0 {
        None
    } else {
        // The leading zeroes count the *additional* bytes that follow the marker bit.
        Some(first_byte.leading_zeros() as usize + 1)
    }
}

/// Decodes an unsigned variable-size integer from a slice of bytes.
///
/// Returns `None` if every data bit is set (the “unknown size” special value).
fn decode_vint_value(bytes: &[u8]) -> Option<u64> {
    let length = bytes.len();
    debug_assert!(
        (1..=MAX_SIZE_LENGTH).contains(&length),
        "vint should occupy between 1 and {MAX_SIZE_LENGTH} bytes"
    );

    let Some((&first_byte, rest)) = bytes.split_first() else {
        return Some(0);
    };

    // Clear the marker bit.
    // All the other bits in the first byte belong to the numeric result.
    let marker = 0x80_u8 >> (length - 1);
    let mut value = u64::from(first_byte & !marker);
    for &byte in rest {
        value = (value << 8_i32) | u64::from(byte);
    }

    // If all the data bits are set, then the size is unknown.
    let data_bits = 7 * length;
    let all_ones = (1_u64 << data_bits) - 1;
    if value == all_ones { None } else { Some(value) }
}

/// Decodes an unsigned variable-size integer from the start of the given byte slice.
///
/// Returns the value together with its size in the source data
/// (i.e. the number of bytes that were actually “consumed”).
///
/// Used for track numbers inside blocks, which are encoded exactly like element sizes.
pub(super) fn decode_vint(bytes: &[u8]) -> Result<(u64, usize), ParseError> {
    let &first_byte = bytes.first().ok_or(ParseError::Truncated)?;
    let length = vint_length(first_byte).ok_or(ParseError::InvalidVint)?;
    if length > MAX_SIZE_LENGTH {
        return Err(ParseError::InvalidVint);
    }

    let slice = bytes.get(..length).ok_or(ParseError::Truncated)?;
    let value = decode_vint_value(slice).ok_or(ParseError::InvalidVint)?;
    Ok((value, length))
}

/// Reads an EBML unsigned integer from the given byte slice.
///
/// Uses all of the bytes in the slice. If it is too large, an `OversizedInteger` error will be returned.
pub(super) fn read_uint(body: &[u8]) -> Result<u64, ParseError> {
    if body.len() > MAX_SIZE_LENGTH {
        return Err(ParseError::OversizedInteger(body.len()));
    }

    let mut value: u64 = 0;
    for &byte in body {
        value = (value << 8_i32) | u64::from(byte);
    }
    Ok(value)
}

/// Reads an EBML string, which may be padded with trailing null bytes so that it can be edited in
/// place without rewriting the file.
pub(super) fn read_string(body: &[u8]) -> Result<String, ParseError> {
    let end = body
        .iter()
        .rposition(|&byte| byte != 0)
        .map_or(0, |index| index + 1);
    let trimmed = body.get(..end).ok_or(ParseError::Truncated)?;
    let parsed = str::from_utf8(trimmed).map_err(ParseError::InvalidUtf8)?;
    Ok(parsed.to_owned())
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ElementHeader {
    pub id: Id,
    pub size: Size,

    /// The offset the header itself starts at,
    /// so that a caller can come back to the element later.
    pub header_start: u64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Element<'a> {
    pub id: Id,
    pub body: &'a [u8],
}

/// Traverses elements held in memory,
/// with their bodies represented as borrowed (sub-)slices.
pub(super) struct Cursor<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    pub(super) const fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ParseError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(ParseError::Truncated)?;
        let slice = self
            .data
            .get(self.position..end)
            .ok_or(ParseError::Truncated)?;
        self.position = end;
        Ok(slice)
    }

    /// Reads the next element (ID and body), or `None` once the end of the underlying data has been reached exactly.
    ///
    /// If a partial element remains at the end, an error is returned.
    pub(super) fn next_element(&mut self) -> Result<Option<Element<'a>>, ParseError> {
        if self.position >= self.data.len() {
            return Ok(None);
        }

        let &first_id_byte = self.data.get(self.position).ok_or(ParseError::Truncated)?;
        let id_length = vint_length(first_id_byte).ok_or(ParseError::InvalidVint)?;
        if id_length > MAX_ID_LENGTH {
            return Err(ParseError::InvalidVint);
        }
        let element_id = Id::from_bytes(self.take(id_length)?);

        let &first_size_byte = self.data.get(self.position).ok_or(ParseError::Truncated)?;
        let size_length = vint_length(first_size_byte).ok_or(ParseError::InvalidVint)?;
        if size_length > MAX_SIZE_LENGTH {
            return Err(ParseError::InvalidVint);
        }

        // An element nested inside a buffered element has no way to express
        // that it “runs to the end of the file”, so an unknown size here would be invalid.
        let size =
            decode_vint_value(self.take(size_length)?).ok_or(ParseError::UnexpectedUnknownSize)?;

        let length = usize::try_from(size).map_err(|_ignored| ParseError::Truncated)?;

        let element = Element {
            id: element_id,
            body: self.take(length)?,
        };
        Ok(Some(element))
    }
}

/// Traverses elements in an async source, one header at a time.
///
/// The reader tracks its own absolute position rather than asking the underlying source, so that
/// buffering stays transparent and offsets recorded during one pass stay valid for the next.
pub(super) struct Reader<R> {
    inner: smol::io::BufReader<R>,
    position: u64,
    scratch: Vec<u8>,
}

impl<R: smol::io::AsyncRead + smol::io::AsyncSeek + Unpin> Reader<R> {
    pub(super) fn new(source: R) -> Self {
        Self {
            inner: smol::io::BufReader::new(source),
            position: 0,
            scratch: vec![],
        }
    }

    pub(super) const fn position(&self) -> u64 {
        self.position
    }

    async fn read_into(&mut self, buffer: &mut [u8]) -> Result<(), ParseError> {
        self.inner
            .read_exact(buffer)
            .await
            .map_err(ParseError::Io)?;
        self.position += buffer.len() as u64;
        Ok(())
    }

    /// Reads `length` bytes into a fresh buffer. Refuses lengths beyond
    /// [`MAX_BUFFERED_ELEMENT_SIZE`], so that a corrupt (or malicious) header cannot trigger a huge allocation.
    pub(super) async fn read_vec(&mut self, length: u64) -> Result<Vec<u8>, ParseError> {
        if length > MAX_BUFFERED_ELEMENT_SIZE {
            return Err(ParseError::ElementTooLarge(length));
        }

        #[expect(clippy::cast_possible_truncation, reason = "64 bit only")]
        let mut buffer = vec![0_u8; length as usize];
        self.read_into(&mut buffer).await?;
        Ok(buffer)
    }

    pub(super) async fn read_vec_size(&mut self, amount: Size) -> Result<Vec<u8>, ParseError> {
        match amount {
            Size::Known(value) => self.read_vec(value).await,
            Size::Unknown => Err(ParseError::UnexpectedUnknownSize),
        }
    }

    /// Reads the next element header. Returns `None` if the source ended exactly on an element
    /// boundary. A partial header is reported as an error.
    pub(super) async fn next_header(&mut self) -> Result<Option<ElementHeader>, ParseError> {
        let header_start = self.position;

        let mut first_id_byte = [0_u8; 1];
        if let Err(error) = self.inner.read_exact(&mut first_id_byte).await {
            return if error.kind() == smol::io::ErrorKind::UnexpectedEof {
                Ok(None)
            } else {
                Err(ParseError::Io(error))
            };
        }
        self.position += 1;

        let id_length = vint_length(first_id_byte[0]).ok_or(ParseError::InvalidVint)?;
        if id_length > MAX_ID_LENGTH {
            return Err(ParseError::InvalidVint);
        }
        let mut id_bytes = [0_u8; MAX_ID_LENGTH];
        id_bytes[0] = first_id_byte[0];
        let id_rest = id_bytes
            .get_mut(1..id_length)
            .ok_or(ParseError::InvalidVint)?;
        self.read_into(id_rest).await?;
        let element_id = Id::from_bytes(id_bytes.get(..id_length).ok_or(ParseError::InvalidVint)?);

        let mut first_size_byte = [0_u8; 1];
        self.read_into(&mut first_size_byte).await?;
        let size_length = vint_length(first_size_byte[0]).ok_or(ParseError::InvalidVint)?;
        if size_length > MAX_SIZE_LENGTH {
            return Err(ParseError::InvalidVint);
        }
        let mut size_bytes = [0_u8; MAX_SIZE_LENGTH];
        size_bytes[0] = first_size_byte[0];
        let size_rest = size_bytes
            .get_mut(1..size_length)
            .ok_or(ParseError::InvalidVint)?;
        self.read_into(size_rest).await?;
        let size = Size::from_bytes(
            size_bytes
                .get(..size_length)
                .ok_or(ParseError::InvalidVint)?,
        );

        Ok(Some(ElementHeader {
            id: element_id,
            size,
            header_start,
        }))
    }

    /// Advances by `amount` bytes without interpreting them.
    pub(super) async fn skip(&mut self, amount: u64) -> Result<(), ParseError> {
        if amount > SEEK_THRESHOLD {
            let target = self
                .position
                .checked_add(amount)
                .ok_or(ParseError::Truncated)?;
            return self.seek_to(target).await;
        }

        #[expect(clippy::cast_possible_truncation, reason = "64 bit only")]
        let length = amount as usize;
        if self.scratch.len() < length {
            self.scratch.resize(length, 0_u8);
        }
        let slice = self
            .scratch
            .get_mut(..length)
            .expect("scratch was just grown to at least this length");
        self.inner.read_exact(slice).await.map_err(ParseError::Io)?;
        self.position += amount;
        Ok(())
    }

    pub(super) async fn skip_size(&mut self, amount: Size) -> Result<(), ParseError> {
        match amount {
            Size::Known(value) => self.skip(value).await,
            Size::Unknown => Err(ParseError::UnexpectedUnknownSize),
        }
    }

    pub(super) async fn seek_to(&mut self, position: u64) -> Result<(), ParseError> {
        self.inner
            .seek(smol::io::SeekFrom::Start(position))
            .await
            .map_err(ParseError::Io)?;
        self.position = position;
        Ok(())
    }
}

#[derive(Error, Debug)]
pub(crate) enum ParseError {
    #[error("I/O error while reading EBML data: {0:?}")]
    Io(smol::io::Error),

    #[error("Malformed variable-size integer")]
    InvalidVint,

    #[error("Element data ended unexpectedly")]
    Truncated,

    #[error("Nested element declared an unknown size")]
    UnexpectedUnknownSize,

    #[error("Integer element is {0} bytes long, which exceeds the 8 byte maximum")]
    OversizedInteger(usize),

    #[error("Refusing to buffer an element of {0} bytes")]
    ElementTooLarge(u64),

    #[error("Invalid UTF-8 in string element: {0:?}")]
    InvalidUtf8(std::str::Utf8Error),
}

#[cfg(test)]
mod tests {
    use super::super::id;
    use super::*;
    use assert_matches2::assert_matches;

    #[test]
    fn vint_lengths() {
        // The marker bit's position gives the width.
        assert_eq!(vint_length(0x80), Some(1));
        assert_eq!(vint_length(0x40), Some(2));
        assert_eq!(vint_length(0x1A), Some(4)); // leading byte of `Segment`/`EBML` style IDs
        assert_eq!(vint_length(0x01), Some(8));
        assert_eq!(vint_length(0xFF), Some(1));
        assert_eq!(vint_length(0x00), None);
    }

    #[test]
    fn id_from_bytes() {
        assert_eq!(Id::from_bytes(&[0x1A, 0x45, 0xDF, 0xA3]), id::EBML);
        assert_eq!(Id::from_bytes(&[0x18, 0x53, 0x80, 0x67]), id::SEGMENT);
        assert_eq!(Id::from_bytes(&[0xA1]), id::BLOCK);
        assert_eq!(Id::from_bytes(&[0x63, 0xA2]), id::CODEC_PRIVATE);
    }

    #[test]
    fn size_from_bytes() {
        assert_eq!(Size::from_bytes(&[0x80]), Size::Known(0));
        assert_eq!(Size::from_bytes(&[0x81]), Size::Known(1));
        assert_eq!(Size::from_bytes(&[0xFE]), Size::Known(126));
        assert_eq!(Size::from_bytes(&[0x40, 0x7F]), Size::Known(127));
        assert_eq!(Size::from_bytes(&[0x41, 0x00]), Size::Known(256));
        assert_eq!(
            Size::from_bytes(&[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
            Size::Known(0)
        );

        // All data bits set means "unknown size", at every possible width.
        assert_eq!(Size::from_bytes(&[0xFF]), Size::Unknown);
        assert_eq!(Size::from_bytes(&[0x7F, 0xFF]), Size::Unknown);
        assert_eq!(
            Size::from_bytes(&[0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]),
            Size::Unknown
        );
    }

    #[test]
    fn vint_with_length() {
        // Track numbers inside blocks are encoded just like sizes, and the caller needs to know
        // how far to advance afterwards.
        assert_matches!(decode_vint(&[0x81, 0xAA, 0xBB]), Ok((1, 1)));
        assert_matches!(decode_vint(&[0x40, 0x7F, 0xCC]), Ok((127, 2)));
        assert_matches!(decode_vint(&[0x00]), Err(ParseError::InvalidVint));
        assert_matches!(decode_vint(&[]), Err(ParseError::Truncated));
        // Declares two bytes but only one is present.
        assert_matches!(decode_vint(&[0x40]), Err(ParseError::Truncated));
    }

    #[test]
    fn integers_and_strings() {
        assert_matches!(read_uint(&[]), Ok(0));
        assert_matches!(read_uint(&[0x01, 0x00]), Ok(256));
        assert_matches!(read_uint(&[0x0F, 0x42, 0x40]), Ok(1_000_000));
        assert_matches!(read_uint(&[0_u8; 9]), Err(ParseError::OversizedInteger(9)));

        assert_eq!(read_string(b"matroska").unwrap(), "matroska");
        // Null padding is stripped, but interior content is preserved.
        assert_eq!(read_string(b"font/ttf\0\0\0").unwrap(), "font/ttf");
        assert_eq!(read_string(b"").unwrap(), "");
        assert_eq!(read_string(b"\0\0").unwrap(), "");
        assert_matches!(read_string(&[0xFF, 0xFE]), Err(ParseError::InvalidUtf8(_)));
    }

    #[test]
    fn cursor() {
        // Two elements (`FileName` = "a", `FileUID` = 1)
        let data = [0x46, 0x6E, 0x81, b'a', 0x46, 0xAE, 0x81, 0x01];
        let mut cursor = Cursor::new(&data);

        assert_matches!(cursor.next_element(), Ok(Some(element)));
        assert_eq!(element.id, id::FILE_NAME);
        assert_eq!(element.body, b"a");
        assert_matches!(cursor.next_element(), Ok(Some(element)));
        assert_eq!(element.id, Id(0x46AE));
        assert_eq!(read_uint(element.body).unwrap(), 1);
        assert_matches!(cursor.next_element(), Ok(None));

        // Declare a four byte body but only provide two
        let data = [0x46, 0x6E, 0x84, b'a', b'b'];
        let mut cursor = Cursor::new(&data);
        assert_matches!(cursor.next_element(), Err(ParseError::Truncated));
    }

    #[test]
    fn reader() {
        // Create data with a 4000 byte element in the middle, to test the read + discard path
        let mut data: Vec<u8> = vec![0xE7, 0x81, 0x05];
        data.extend_from_slice(&[0xEC, 0x4F, 0xA0]); // Void, size 4000 (two byte vint)
        data.extend(std::iter::repeat_n(0_u8, 4000));
        data.extend_from_slice(&[0xE7, 0x81, 0x06]);

        smol::block_on(async {
            let mut reader = Reader::new(smol::io::Cursor::new(data));

            let first = reader.next_header().await.unwrap().unwrap();
            assert_eq!(first.id, id::TIMESTAMP);
            assert_eq!(first.header_start, 0);
            let body = reader.read_vec_size(first.size).await.unwrap();
            assert_eq!(read_uint(&body).unwrap(), 5);

            let void = reader.next_header().await.unwrap().unwrap();
            assert_eq!(void.size, Size::Known(4000));
            reader.skip_size(void.size).await.unwrap();

            let last = reader.next_header().await.unwrap().unwrap();
            assert_eq!(last.id, id::TIMESTAMP);

            // Make sure the position is tracked correctly after the skip
            assert_eq!(last.header_start, 4006);

            let body = reader.read_vec_size(last.size).await.unwrap();
            assert_eq!(read_uint(&body).unwrap(), 6);

            assert_matches!(reader.next_header().await, Ok(None));
        });
    }

    #[test]
    fn reader_seek_back() {
        let data: Vec<u8> = vec![0xE7, 0x81, 0x05, 0xE7, 0x81, 0x06];

        smol::block_on(async {
            let mut reader = Reader::new(smol::io::Cursor::new(data));

            let first = reader.next_header().await.unwrap().unwrap();
            reader.skip_size(first.size).await.unwrap();
            let second = reader.next_header().await.unwrap().unwrap();
            reader.skip_size(second.size).await.unwrap();
            assert_matches!(reader.next_header().await, Ok(None));

            // Returning to a recorded offset must yield the same element again.
            reader.seek_to(second.header_start).await.unwrap();
            let again = reader.next_header().await.unwrap().unwrap();
            assert_eq!(again.header_start, second.header_start);
            let body = reader.read_vec_size(again.size).await.unwrap();
            assert_eq!(read_uint(&body).unwrap(), 6);
        });
    }

    #[test]
    fn unknown_size() {
        // `Segment` with an unknown size, like when muxing a live stream.
        let data: Vec<u8> = vec![0x18, 0x53, 0x80, 0x67, 0xFF];

        smol::block_on(async {
            let mut reader = Reader::new(smol::io::Cursor::new(data));
            let header = reader.next_header().await.unwrap().unwrap();
            assert_eq!(header.id, id::SEGMENT);
            assert_eq!(header.size, Size::Unknown);
        });
    }

    #[test]
    fn partial_header() {
        // An ID byte promising three more bytes, with nothing following it.
        let data: Vec<u8> = vec![0x1A];

        smol::block_on(async {
            let mut reader = Reader::new(smol::io::Cursor::new(data));
            assert_matches!(reader.next_header().await, Err(ParseError::Io(_)));
        });
    }
}
