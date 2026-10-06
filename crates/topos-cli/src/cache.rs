//! Cached results per file (issue #2), for documents that rarely change.
//!
//! - Results are cached **before filtering**: filters only drop matches after they are found, so
//!   any filter can reuse them (see `BibleMatcher::without_filters`). Only options that change
//!   what is found (custom data, book context, the version) get a separate cache.
//! - Each searched file has its own small entry (postcard, a compact binary format), so a run
//!   reads only the entries it needs, in parallel, and an edit rewrites one entry.
//! - An entry starts with the set of books its file mentions, so a search for other books skips
//!   decoding its results.

use std::{
    collections::BTreeSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use topos_lib::data::books::BookId;

use crate::search::Hit;

/// Bumped when entries change shape, so old ones are never misread
const FORMAT: u32 = 2;
/// Prune entries for deleted files at most this often
const PRUNE_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Caches for other options (an old version, a data file no longer used) are removed after this
const UNUSED_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// What an entry starts with: enough to check it is current and whether it can match
#[derive(Serialize, Deserialize)]
struct Header {
    /// The canonical path, since entries are named by a hash of it
    path: PathBuf,
    size: u64,
    modified_ns: u128,
    /// Bit `n` is set if the file mentions book `n`
    books: [u64; 4],
}

/// A cache lookup
pub enum Cached {
    /// The file's unfiltered results
    Hits(Vec<Hit>),
    /// The file mentions none of the books being searched for, so nothing in it can match
    NoMatch,
}

pub struct Cache {
    /// `~/.cache/topos/v2/<options>/`
    dir: PathBuf,
}

/// `$XDG_CACHE_HOME/topos`, else `~/.cache/topos`
pub fn root() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    Some(base.join("topos"))
}

/// FNV-1a: a hash that is the same in every build, unlike `DefaultHasher`
fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

impl Cache {
    /// `fingerprint` describes every option that changes which references are found
    pub fn open(fingerprint: &str) -> Option<Self> {
        let root = root()?;
        remove_legacy(&root);
        let key = format!("{}\0{FORMAT}\0{fingerprint}", env!("CARGO_PKG_VERSION"));
        let dir = root
            .join(format!("v{FORMAT}"))
            .join(format!("{:016x}", stable_hash(key.as_bytes())));
        fs::create_dir_all(&dir).ok()?;
        // Marks this cache as used, for pruning other ones
        let _ = fs::write(dir.join(".used"), "");
        Some(Self { dir })
    }

    fn entry(&self, key: &Path) -> PathBuf {
        let hash = stable_hash(key.as_os_str().as_encoded_bytes());
        self.dir.join(format!("{hash:016x}.bin"))
    }

    /**
    The file's cached results, if it has not changed since they were stored
    - With `books` (the only books a match could be in), a file that mentions none of them is
      [`Cached::NoMatch`] without decoding its results
    */
    pub fn get(&self, path: &Path, books: Option<&BTreeSet<BookId>>) -> Option<Cached> {
        let (key, size, modified_ns) = stamp(path)?;
        let bytes = fs::read(self.entry(&key)).ok()?;
        let (header, rest) = postcard::take_from_bytes::<Header>(&bytes).ok()?;
        if header.path != key || header.size != size || header.modified_ns != modified_ns {
            return None;
        }
        if let Some(books) = books
            && !books.iter().any(|book| has_book(&header.books, *book))
        {
            return Some(Cached::NoMatch);
        }
        postcard::from_bytes(rest).ok().map(Cached::Hits)
    }

    /// Stores a file's unfiltered results (errors are ignored: the cache is only an optimization)
    pub fn insert(&self, path: &Path, hits: &[Hit]) {
        let Some((key, size, modified_ns)) = stamp(path) else {
            return;
        };
        let mut books = [0; 4];
        for hit in hits {
            let id = usize::from(hit.passage.book.0);
            books[id / 64] |= 1 << (id % 64);
        }
        let header = Header {
            path: key.clone(),
            size,
            modified_ns,
            books,
        };
        let Ok(mut bytes) = postcard::to_allocvec(&header) else {
            return;
        };
        let Ok(body) = postcard::to_allocvec(hits) else {
            return;
        };
        bytes.extend(body);
        let _ = write_atomically(&self.entry(&key), &bytes);
    }

    /// After a run: now and then, removes entries for files that no longer exist, and caches
    /// for options that haven't been used in a month
    pub fn finish(&self) {
        let marker = self.dir.join(".pruned");
        if modified_within(&marker, PRUNE_EVERY) {
            return;
        }
        let _ = fs::write(&marker, "");
        for entry in fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "bin") {
                continue;
            }
            let source = fs::read(&path)
                .ok()
                .and_then(|bytes| Some(postcard::take_from_bytes::<Header>(&bytes).ok()?.0.path));
            if source.is_none_or(|source| !source.exists()) {
                let _ = fs::remove_file(path);
            }
        }
        let Some(versions) = self.dir.parent() else {
            return;
        };
        for other in fs::read_dir(versions).into_iter().flatten().flatten() {
            let other = other.path();
            if other != self.dir && !modified_within(&other.join(".used"), UNUSED_FOR) {
                let _ = fs::remove_dir_all(other);
            }
        }
    }
}

/// Deletes the whole cache (`--clear-cache`)
pub fn clear() -> io::Result<()> {
    match root() {
        Some(root) if root.exists() => fs::remove_dir_all(root),
        _ => Ok(()),
    }
}

fn has_book(books: &[u64; 4], book: BookId) -> bool {
    let id = usize::from(book.0);
    books[id / 64] & (1 << (id % 64)) != 0
}

fn stamp(path: &Path) -> Option<(PathBuf, u64, u128)> {
    let key = fs::canonicalize(path).ok()?;
    let meta = fs::metadata(&key).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some((key, meta.len(), modified.as_nanos()))
}

fn modified_within(path: &Path, age: Duration) -> bool {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|elapsed| elapsed < age)
}

/// Writes to a unique temporary file, then renames it, so readers never see half an entry
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_extension(format!("tmp.{}.{n}", std::process::id()));
    let mut file = fs::File::create(&temp)?;
    file.write_all(bytes)?;
    drop(file);
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

/// The first cache was one JSON file per set of options, directly in the cache folder
fn remove_legacy(root: &Path) {
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_file() && (name.ends_with(".json") || name.ends_with(".json.tmp")) {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_hash_is_fnv1a() {
        assert_eq!(stable_hash(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(stable_hash(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn book_bits() {
        let mut books = [0; 4];
        for id in [1u8, 64, 66, 255] {
            books[usize::from(id) / 64] |= 1 << (usize::from(id) % 64);
        }
        assert!(has_book(&books, BookId(66)));
        assert!(has_book(&books, BookId(255)));
        assert!(!has_book(&books, BookId(2)));
    }
}
