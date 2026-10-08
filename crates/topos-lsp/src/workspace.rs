//! Searching the whole workspace: every text file under the workspace folders (respecting
//! `.gitignore`, `.ignore`, and `.toposignore`, and skipping hidden and binary files, like the
//! CLI), with open documents' unsaved text instead of what is on disk, and a cache of each
//! file's references.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
    time::SystemTime,
};

use ignore::WalkState;
use lsp_types::{Range, Uri};
use topos_bible::{
    matcher::{BibleMatcher, LineIndex},
    segments::Passage,
};
use topos_bible_index::files::WalkOptions;

/// Larger files are skipped (a book-length text is a few MB)
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// A `file://` URI for a path, percent-encoding what URIs can't hold (like spaces)
pub fn path_to_uri(path: &Path) -> Option<Uri> {
    let path = path.to_str()?;
    let mut encoded = String::from("file://");
    if !path.starts_with('/') {
        // Windows: file:///C:/...
        encoded.push('/');
    }
    for byte in path.replace('\\', "/").bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    Uri::from_str(&encoded).ok()
}

/// The path of a `file://` URI
pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let text = uri.as_str().strip_prefix("file://")?;
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let path = String::from_utf8(decoded).ok()?;
    // Windows: /C:/... is C:/...
    let path = match path.as_bytes() {
        [b'/', _, b':', ..] => path[1..].to_string(),
        _ => path,
    };
    Some(PathBuf::from(path))
}

/// A reference found in a file: where it is (in LSP positions) and what it is
#[derive(Clone, Debug)]
pub struct Found {
    pub range: Range,
    pub passage: Passage,
}

/// Each file's references, found once and reused while the file is unchanged
struct Entry {
    size: u64,
    modified: Option<SystemTime>,
    found: Arc<Vec<Found>>,
}

/**
A cache of every workspace file's references, before any filter (searches filter them), so a
search reads and parses only the files that changed since the last one
- Files are matched by size and modification time, like the CLI's `--cache`
- [`Cache::clear`] when the data changes, since that changes what is found
*/
#[derive(Default)]
pub struct Cache {
    files: Mutex<HashMap<PathBuf, Entry>>,
}

impl Cache {
    pub fn clear(&self) {
        if let Ok(mut files) = self.files.lock() {
            files.clear();
        }
    }

    fn get(&self, path: &Path, size: u64, modified: Option<SystemTime>) -> Option<Arc<Vec<Found>>> {
        let files = self.files.lock().ok()?;
        let entry = files.get(path)?;
        (entry.size == size && entry.modified == modified).then(|| entry.found.clone())
    }

    fn insert(
        &self,
        path: PathBuf,
        size: u64,
        modified: Option<SystemTime>,
        found: Arc<Vec<Found>>,
    ) {
        if let Ok(mut files) = self.files.lock() {
            files.insert(
                path,
                Entry {
                    size,
                    modified,
                    found,
                },
            );
        }
    }

    /// Forgets files that are gone (or no longer searched)
    fn retain(&self, seen: &HashSet<PathBuf>) {
        if let Ok(mut files) = self.files.lock() {
            files.retain(|path, _| seen.contains(path));
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.files.lock().map_or(0, |files| files.len())
    }
}

/// The references in a text, with LSP ranges
pub fn find(matcher: &BibleMatcher, text: &str) -> Vec<Found> {
    let index = LineIndex::new(text);
    matcher
        .search(text)
        .into_iter()
        .map(|m| {
            let bytes = m.location.bytes;
            Found {
                range: crate::server::lsp_range(&index, bytes.start, bytes.end),
                passage: m.psg,
            }
        })
        .collect()
}

/**
Every reference in the workspace, by file: open documents as they are in the editor (unsaved
text included), then the files under `roots` with one of `extensions` (any if empty), from the
cache when unchanged. Files are walked in parallel
*/
// `Uri` caches its parsed parts, which clippy counts as a mutable key; its text never changes
#[allow(clippy::mutable_key_type)]
pub fn references(
    roots: &[PathBuf],
    documents: &HashMap<Uri, String>,
    extensions: &[String],
    matcher: &BibleMatcher,
    cache: &Cache,
) -> Vec<(Uri, Arc<Vec<Found>>)> {
    // Open documents by path: editors may percent-encode their URIs differently
    let open: HashSet<PathBuf> = documents.keys().filter_map(uri_to_path).collect();
    // Unnamed documents (`file://`) are left out: there is no file to go to
    let results: Mutex<Vec<(Uri, Arc<Vec<Found>>)>> = Mutex::new(
        documents
            .iter()
            .filter(|(uri, _)| !crate::server::is_unnamed(uri))
            .map(|(uri, text)| (uri.clone(), Arc::new(find(matcher, text))))
            .collect(),
    );
    let seen: Mutex<HashSet<PathBuf>> = Mutex::new(HashSet::new());
    // The CLI's walking rules (ignore files, `.toposignore`, hidden files)
    let walk = WalkOptions {
        max_filesize: Some(MAX_FILE_SIZE),
        ..WalkOptions::default()
    };
    let Ok(builder) = walk.builder(roots) else {
        return results.into_inner().unwrap_or_default();
    };
    builder.build_parallel().run(|| {
        Box::new(|entry| {
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            let path = entry.path();
            let wanted = entry.file_type().is_some_and(|t| t.is_file())
                && !open.contains(path)
                && (extensions.is_empty()
                    || path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e))));
            if !wanted {
                return WalkState::Continue;
            }
            let (Some(uri), Ok(meta)) = (path_to_uri(path), entry.metadata()) else {
                return WalkState::Continue;
            };
            if let Ok(mut seen) = seen.lock() {
                seen.insert(path.to_path_buf());
            }
            let (size, modified) = (meta.len(), meta.modified().ok());
            let found = match cache.get(path, size, modified) {
                Some(found) => found,
                None => {
                    let Ok(bytes) = std::fs::read(path) else {
                        return WalkState::Continue;
                    };
                    // Like ripgrep, skip files that look binary (cached as having none)
                    let found = if bytes[..bytes.len().min(8192)].contains(&0) {
                        Arc::new(vec![])
                    } else {
                        Arc::new(find(matcher, &String::from_utf8_lossy(&bytes)))
                    };
                    cache.insert(path.to_path_buf(), size, modified, found.clone());
                    found
                }
            };
            if !found.is_empty()
                && let Ok(mut results) = results.lock()
            {
                results.push((uri, found));
            }
            WalkState::Continue
        })
    });
    cache.retain(&seen.into_inner().unwrap_or_default());
    results.into_inner().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip() {
        let path = Path::new("/home/me/My Notes/Romans 8 (2025)#1.md");
        let uri = path_to_uri(path).unwrap();
        assert_eq!(
            uri.as_str(),
            "file:///home/me/My%20Notes/Romans%208%20%282025%29%231.md"
        );
        assert_eq!(uri_to_path(&uri).unwrap(), path);
    }
}
