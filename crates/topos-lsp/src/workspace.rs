//! Searching the whole workspace: every text file under the workspace folders (respecting
//! `.gitignore`, `.ignore`, and `.toposignore`, and skipping hidden and binary files, like the
//! CLI), with open documents' unsaved text instead of what is on disk.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    str::FromStr,
};

use ignore::WalkBuilder;
use lsp_types::Uri;

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

/// The text of every file to search: open documents first (their unsaved text), then the files
/// under `roots` with one of `extensions` (any extension if empty)
// `Uri` caches its parsed parts, which clippy counts as a mutable key; its text never changes
#[allow(clippy::mutable_key_type)]
pub fn texts(
    roots: &[PathBuf],
    documents: &HashMap<Uri, String>,
    extensions: &[String],
) -> Vec<(Uri, String)> {
    // Open documents by path: editors may percent-encode their URIs differently
    let open: HashSet<PathBuf> = documents.keys().filter_map(uri_to_path).collect();
    let mut texts: Vec<(Uri, String)> = documents
        .iter()
        .map(|(uri, text)| (uri.clone(), text.clone()))
        .collect();
    let Some((first, rest)) = roots.split_first() else {
        return texts;
    };
    let mut builder = WalkBuilder::new(first);
    for root in rest {
        builder.add(root);
    }
    builder
        .add_custom_ignore_filename(".toposignore")
        .max_filesize(Some(MAX_FILE_SIZE));
    for entry in builder.build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let extension = path.extension().and_then(|e| e.to_str());
        if !extensions.is_empty()
            && !extension.is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
        {
            continue;
        }
        if open.contains(path) {
            continue;
        }
        let Some(uri) = path_to_uri(path) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        // Like ripgrep, skip files that look binary
        if bytes[..bytes.len().min(8192)].contains(&0) {
            continue;
        }
        texts.push((uri, String::from_utf8_lossy(&bytes).into_owned()));
    }
    texts
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
