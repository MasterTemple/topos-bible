//! The binary formats: an 8-byte magic, a format version, whether it's compressed, then postcard
//! (compressed with deflate for EPUB details)
//!
//! - A **pack** holds one device's entries for the paths that hash to one pack number, so an edit
//!   rewrites one small file, and devices never write the same file (syncing can't conflict)
//! - A **detail** holds one EPUB's CFIs and context, loaded only when needed
//! - An **entry** is one file's entry (and details), as a background thread or the CLI sends it

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::entry::{Detail, FileEntry, Unit};

/// Bumped when any format changes shape, so old files are never misread
pub const FORMAT: u16 = 2;

/// Deflate's level: fast to write (an edit rewrites a pack), and most of the size saved
const LEVEL: u8 = 1;

const PACK: &[u8; 8] = b"TOPOSPAK";
const DETAIL: &[u8; 8] = b"TOPOSDTL";
const ENTRY: &[u8; 8] = b"TOPOSENT";

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum FormatError {
    #[error("not a topos index file")]
    NotIndex,
    #[error("an index file from another version of topos (format {0}, this one reads {FORMAT})")]
    Version(u16),
    #[error("a damaged index file: {0}")]
    Damaged(String),
}

/// A device's confirmation that another device's entry matches its copy of a file (which may
/// have another modification time after syncing)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confirmation {
    /// The file's size and modification time on this device
    pub size: u64,
    pub mtime: u64,
    /// The confirmed entry: its device and [`FileEntry::written`]
    pub device: String,
    pub written: u64,
}

/// One device's entries for one pack number
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pack {
    pub device: String,
    pub number: u32,
    /// How many packs the device splits paths into
    pub count: u32,
    pub unit: Unit,
    /// What wrote it (a version), for information
    pub engine: String,
    pub entries: Vec<(String, FileEntry)>,
    pub confirmations: Vec<(String, Confirmation)>,
}

/// One file's entry and details, sent between threads or from the CLI
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryMessage {
    pub path: String,
    pub unit: Unit,
    pub entry: FileEntry,
    pub detail: Option<Detail>,
}

fn encode<T: Serialize>(magic: &[u8; 8], value: &T, compress: bool) -> Vec<u8> {
    // These types always serialize; if one didn't, the file would read as damaged, which only
    // means its files get searched again
    let body = postcard::to_stdvec(value).unwrap_or_default();
    let mut out = Vec::with_capacity(body.len() / 2 + 11);
    out.extend_from_slice(magic);
    out.extend_from_slice(&FORMAT.to_le_bytes());
    out.push(u8::from(compress));
    if compress {
        out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec(&body, LEVEL));
    } else {
        out.extend_from_slice(&body);
    }
    out
}

fn decode<T: DeserializeOwned>(magic: &[u8; 8], bytes: &[u8]) -> Result<T, FormatError> {
    if bytes.len() < 11 || &bytes[..8] != magic {
        return Err(FormatError::NotIndex);
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version != FORMAT {
        return Err(FormatError::Version(version));
    }
    let damaged = |e: String| FormatError::Damaged(e);
    let body = &bytes[11..];
    let inflated;
    let body = if bytes[10] == 1 {
        inflated =
            miniz_oxide::inflate::decompress_to_vec(body).map_err(|e| damaged(e.to_string()))?;
        &inflated[..]
    } else {
        body
    };
    postcard::from_bytes(body).map_err(|e| damaged(e.to_string()))
}

impl Pack {
    /// Not compressed: references are already compact, and reading them at startup is what
    /// matters most (inflating took two thirds of the time for a third of the size)
    pub fn encode(&self) -> Vec<u8> {
        encode(PACK, self, false)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, FormatError> {
        decode(PACK, bytes)
    }
}

impl Detail {
    pub fn encode(&self) -> Vec<u8> {
        encode(DETAIL, self, true)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, FormatError> {
        decode(DETAIL, bytes)
    }
}

impl EntryMessage {
    /// Not compressed: these are read right away
    pub fn encode(&self) -> Vec<u8> {
        encode(ENTRY, self, false)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, FormatError> {
        decode(ENTRY, bytes)
    }
}

/// Which pack a path's entry goes in (FNV-1a of the path)
pub fn pack_number(path: &str, count: u32) -> u32 {
    (crate::entry::text_hash(path) % u64::from(count.max(1))) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Stamp;

    #[test]
    fn round_trips_and_rejects_others() {
        let entry = FileEntry::new(
            Stamp {
                size: 3,
                mtime: 4,
                hash: Some(5),
            },
            6,
            vec![],
        );
        let pack = Pack {
            device: "laptop".into(),
            number: 1,
            count: 32,
            unit: Unit::Utf16,
            engine: "test".into(),
            entries: vec![("a.md".into(), entry.clone())],
            confirmations: vec![],
        };
        let bytes = pack.encode();
        assert_eq!(Pack::decode(&bytes), Ok(pack));
        assert_eq!(Detail::decode(&bytes), Err(FormatError::NotIndex));
        let mut old = bytes.clone();
        old[8] = 0;
        assert_eq!(Pack::decode(&old), Err(FormatError::Version(0)));
        assert!(matches!(
            Pack::decode(&bytes[..bytes.len() - 3]),
            Err(FormatError::Damaged(_))
        ));
        let message = EntryMessage {
            path: "a.md".into(),
            unit: Unit::Utf16,
            entry,
            detail: None,
        };
        assert_eq!(EntryMessage::decode(&message.encode()), Ok(message));
    }
}
