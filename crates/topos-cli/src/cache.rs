//! Cached results per file (issue #2), for documents that rarely change.

use std::{
    collections::HashMap,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    sync::Mutex,
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

use crate::search::Hit;

/// Results for one file, valid while its size and modification time are unchanged
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    size: u64,
    modified_ns: u128,
    hits: Vec<Hit>,
}

/**
- One cache file per set of options that change results (filters, context, config, version),
  so different searches never share entries
- Entries are keyed by canonical path, so symlinks and relative paths share one entry
*/
pub struct Cache {
    file: PathBuf,
    entries: Mutex<HashMap<PathBuf, Entry>>,
    changed: Mutex<bool>,
}

impl Cache {
    /// `fingerprint` describes every option that changes results
    pub fn open(fingerprint: &str) -> Option<Self> {
        let dir = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?
            .join("topos");
        let mut hasher = DefaultHasher::new();
        (env!("CARGO_PKG_VERSION"), fingerprint).hash(&mut hasher);
        let file = dir.join(format!("{:016x}.json", hasher.finish()));
        let entries = fs::read_to_string(&file)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Some(Self {
            file,
            entries: Mutex::new(entries),
            changed: Mutex::new(false),
        })
    }

    /// The cached hits, if the file has not changed since they were stored
    pub fn get(&self, path: &Path) -> Option<Vec<Hit>> {
        let (key, size, modified_ns) = Self::stamp(path)?;
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(&key)?;
        (entry.size == size && entry.modified_ns == modified_ns).then(|| entry.hits.clone())
    }

    pub fn insert(&self, path: &Path, hits: &[Hit]) {
        let Some((key, size, modified_ns)) = Self::stamp(path) else {
            return;
        };
        if let (Ok(mut entries), Ok(mut changed)) = (self.entries.lock(), self.changed.lock()) {
            let entry = Entry {
                size,
                modified_ns,
                hits: hits.to_vec(),
            };
            entries.insert(key, entry);
            *changed = true;
        }
    }

    /// Writes the cache if anything was added
    pub fn save(&self) -> std::io::Result<()> {
        if !self.changed.lock().is_ok_and(|changed| *changed) {
            return Ok(());
        }
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir)?;
        }
        let entries = self
            .entries
            .lock()
            .map_err(|_| std::io::Error::other("poisoned"))?;
        let json = serde_json::to_string(&*entries).map_err(std::io::Error::other)?;
        // Write then rename, so a concurrent run never reads half a file
        let temp = self.file.with_extension("json.tmp");
        fs::write(&temp, json)?;
        fs::rename(temp, &self.file)
    }

    fn stamp(path: &Path) -> Option<(PathBuf, u64, u128)> {
        let key = fs::canonicalize(path).ok()?;
        let meta = fs::metadata(&key).ok()?;
        let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        Some((key, meta.len(), modified.as_nanos()))
    }
}
