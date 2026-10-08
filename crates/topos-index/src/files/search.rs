//! Searching files and folders: which files are walked (like ripgrep), how each format is
//! searched, and the cache

use std::{
    fs,
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread,
};

use super::cache::{Cache, Cached};
use ignore::{WalkBuilder, WalkState, overrides::OverrideBuilder};
use std::collections::BTreeSet;
use topos_bible::{
    data::books::BookId,
    matcher::{BibleMatcher, Position},
    segments::Passage,
};
use topos_bible_formats::{
    epub::{CfiOptions, search_epub},
    srt::{SRTDocument, SRTTimeStamp},
};

/// One reference found in a file or text
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Hit {
    pub passage: Passage,
    /// Start and end positions, for text that has lines
    pub position: Option<(Position, Position)>,
    /// Byte range in the text, for text that has lines
    pub bytes: Option<Range<usize>>,
    /// Where the hit is in other terms, like a timestamp, page, or EPUB CFI
    pub label: Option<String>,
    /// More about where a hit in an EPUB is
    pub epub: Option<EpubPlace>,
}

/// Where a hit in an EPUB is, besides its CFI (for JSON, so the Obsidian plugin can use it)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EpubPlace {
    /// Its content document's position in the spine
    pub spine_index: usize,
    /// The table of contents' name for its content document
    pub chapter: Option<String>,
    /// Its UTF-16 range in the book's text (every content document's, a blank line apart)
    pub text_utf16: Range<usize>,
    /// 1-based, in the book's text
    pub line: usize,
    /// 1-based, in UTF-16 code units
    pub utf16_column: usize,
    /// The line (paragraph) it starts on
    pub line_text: String,
}

/// The hits in one file (or in the text given directly)
#[derive(Clone, Debug)]
pub struct FileHits {
    /// [`None`] for `--text` or stdin
    pub path: Option<PathBuf>,
    /// The searched text, when it has lines (used for context)
    pub text: Option<Arc<str>>,
    pub hits: Vec<Hit>,
}

/// A result for each searched file, or an error with the file it came from
pub type FileResult = Result<FileHits, (Option<PathBuf>, String)>;

/// How files are searched (see [`search`])
pub struct Searcher {
    pub matcher: BibleMatcher,
    /// With a cache: what is searched and stored (`matcher` without filters), and the only books
    /// `matcher` can keep
    pub cached: Option<CachedSearch>,
    /// Keep the text of cached files (for context lines)
    pub needs_text: bool,
    /// Which files are walked
    pub walk: WalkOptions,
    /// Search files that look binary as text
    pub binary: bool,
    /// `--files`: report the files that would be searched (with no hits), without searching
    pub list_only: bool,
    /// How EPUB CFIs are written
    pub cfi: CfiOptions,
}

/// Which files are searched, like ripgrep's options
#[derive(Clone, Debug, Default)]
pub struct WalkOptions {
    pub hidden: bool,
    pub no_ignore: bool,
    pub no_ignore_vcs: bool,
    pub no_ignore_parent: bool,
    pub no_require_git: bool,
    pub ignore_files: Vec<PathBuf>,
    pub follow: bool,
    pub max_depth: Option<usize>,
    pub max_filesize: Option<u64>,
    pub one_file_system: bool,
    /// Globs that files must match (`!` excludes), relative to the current folder
    pub globs: Vec<String>,
    /// Globs like `globs`, matched without regard to case
    pub iglobs: Vec<String>,
    /// Lowercase extensions to search (empty searches every file)
    pub extensions: Vec<String>,
    /// Lowercase extensions to skip
    pub exclude_extensions: Vec<String>,
    /// Only EPUBs, even files named directly (the CLI's `--epub-links`)
    pub only_epub: bool,
}

impl WalkOptions {
    /// A walker over `paths` with these options, like ripgrep's (an error for a bad glob or
    /// ignore file)
    pub fn builder(&self, paths: &[PathBuf]) -> Result<WalkBuilder, String> {
        let Some(first) = paths.first() else {
            return Err("no paths to search".into());
        };
        let mut builder = WalkBuilder::new(first);
        for path in &paths[1..] {
            builder.add(path);
        }
        let vcs = !self.no_ignore && !self.no_ignore_vcs;
        builder
            .hidden(!self.hidden)
            .ignore(!self.no_ignore)
            .git_ignore(vcs)
            .git_global(vcs)
            .git_exclude(vcs)
            .parents(!self.no_ignore && !self.no_ignore_parent)
            .require_git(!self.no_require_git)
            .follow_links(self.follow)
            .max_depth(self.max_depth)
            .max_filesize(self.max_filesize)
            .same_file_system(self.one_file_system);
        if !self.no_ignore {
            builder.add_custom_ignore_filename(".toposignore");
        }
        for file in &self.ignore_files {
            if let Some(err) = builder.add_ignore(file) {
                return Err(format!("{}: {err}", file.display()));
            }
        }
        if !self.globs.is_empty() || !self.iglobs.is_empty() {
            let root = std::env::current_dir().map_err(|e| e.to_string())?;
            let mut overrides = OverrideBuilder::new(root);
            for glob in &self.globs {
                overrides
                    .add(glob)
                    .map_err(|e| format!("--glob {glob}: {e}"))?;
            }
            overrides
                .case_insensitive(true)
                .map_err(|e| e.to_string())?;
            for glob in &self.iglobs {
                overrides
                    .add(glob)
                    .map_err(|e| format!("--iglob {glob}: {e}"))?;
            }
            builder.overrides(overrides.build().map_err(|e| e.to_string())?);
        }
        Ok(builder)
    }

    /// Extensions as `--ext` takes them (`.md`, `MD`) in the form these options keep them
    pub fn extension_list(list: &[String]) -> Vec<String> {
        list.iter()
            .map(|ext| ext.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|ext| !ext.is_empty())
            .collect()
    }

    /// Whether `--ext` and `--exclude-ext` allow this file
    fn wants(&self, path: &Path) -> bool {
        let extension = path.extension().and_then(|e| e.to_str());
        let listed = |list: &[String]| {
            extension.is_some_and(|e| list.iter().any(|ext| ext.eq_ignore_ascii_case(e)))
        };
        (self.extensions.is_empty() || listed(&self.extensions))
            && !listed(&self.exclude_extensions)
    }
}

/// Searching with a cache of unfiltered results
pub struct CachedSearch {
    pub cache: Cache,
    pub unfiltered: BibleMatcher,
    pub possible_books: Option<BTreeSet<BookId>>,
}

impl CachedSearch {
    pub fn new(cache: Cache, matcher: &BibleMatcher) -> Self {
        Self {
            cache,
            unfiltered: matcher.without_filters(),
            possible_books: matcher.possible_books(),
        }
    }
}

/// Searches files and folders (in parallel), sending each file's result as soon as it's ready
pub fn search(searcher: Arc<Searcher>, paths: Vec<PathBuf>) -> mpsc::Receiver<FileResult> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || walk(searcher, paths, sender));
    receiver
}

fn walk(searcher: Arc<Searcher>, paths: Vec<PathBuf>, sender: mpsc::Sender<FileResult>) {
    let builder = match searcher.walk.builder(&paths) {
        Ok(builder) => builder,
        Err(err) => {
            let _ = sender.send(Err((None, err)));
            return;
        }
    };
    builder.build_parallel().run(|| {
        let searcher = searcher.clone();
        let sender = sender.clone();
        Box::new(move |entry| {
            let result = match entry {
                Ok(entry) if entry.file_type().is_some_and(|t| t.is_dir()) => {
                    return WalkState::Continue;
                }
                // Files named on the command line (depth 0) are always searched
                Ok(entry) if entry.depth() > 0 && !searcher.walk.wants(entry.path()) => {
                    return WalkState::Continue;
                }
                Ok(entry) if searcher.walk.only_epub && !is_epub(entry.path()) => {
                    return WalkState::Continue;
                }
                Ok(entry) if searcher.list_only => Ok(FileHits {
                    path: Some(entry.path().to_path_buf()),
                    text: None,
                    hits: vec![],
                }),
                Ok(entry) => match searcher.search_file(entry.path()) {
                    Ok(Some(hits)) => Ok(hits),
                    Ok(None) => return WalkState::Continue,
                    Err(err) => Err((Some(entry.path().to_path_buf()), err)),
                },
                Err(err) => Err(match error_path(&err) {
                    Some((path, inner)) => (Some(path.to_path_buf()), inner.to_string()),
                    None => (None, err.to_string()),
                }),
            };
            match sender.send(result) {
                Ok(()) => WalkState::Continue,
                Err(_) => WalkState::Quit,
            }
        })
    });
}

/// The file a walking error is about, and the error without it
fn error_path(err: &ignore::Error) -> Option<(&Path, &ignore::Error)> {
    match err {
        ignore::Error::WithPath { path, err } => Some((path, err)),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
            error_path(err)
        }
        _ => None,
    }
}

/// Whether a path ends in `.epub` (in any case)
pub fn is_epub(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
}

impl Searcher {
    /**
    Uses the cache when the file has not changed
    - The cache holds unfiltered results, so they are filtered here, whatever the filters
    - New results are found without filters, stored, then filtered
    - Skipped (binary) files are stored with no results, so they aren't read again
    */
    fn search_file(&self, path: &Path) -> Result<Option<FileHits>, String> {
        let Some(cached) = &self.cached else {
            return search_file(&self.matcher, path, self.binary, self.cfi);
        };
        let keep = |hits: Vec<Hit>| -> Vec<Hit> {
            hits.into_iter()
                .filter(|hit| self.matcher.keeps(&hit.passage))
                .collect()
        };
        match cached.cache.get(path, cached.possible_books.as_ref()) {
            // No hits, but still a searched file (for --files-without-match)
            Some(Cached::NoMatch) => Ok(Some(FileHits {
                path: Some(path.to_path_buf()),
                text: None,
                hits: vec![],
            })),
            Some(Cached::Hits(hits)) => {
                let hits = keep(hits);
                let text = (self.needs_text && !hits.is_empty())
                    .then(|| fs::read(path).ok())
                    .flatten()
                    .map(|bytes| Arc::from(String::from_utf8_lossy(&bytes).as_ref()));
                let path = Some(path.to_path_buf());
                Ok(Some(FileHits { path, text, hits }))
            }
            None => {
                let found = search_file(&cached.unfiltered, path, self.binary, self.cfi)?;
                cached
                    .cache
                    .insert(path, found.as_ref().map_or(&[], |f| &f.hits));
                Ok(found.map(|found| FileHits {
                    hits: keep(found.hits),
                    ..found
                }))
            }
        }
    }
}

/// Searches one file by its format; `Ok(None)` for a file that is skipped (binary, without a
/// supported format)
pub fn search_file(
    matcher: &BibleMatcher,
    path: &Path,
    binary: bool,
    cfi: CfiOptions,
) -> Result<Option<FileHits>, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let path_buf = Some(path.to_path_buf());
    match extension.as_deref() {
        Some("epub") => {
            let matches = search_epub(matcher, path, cfi).map_err(|e| e.to_string())?;
            let hits = matches
                .into_iter()
                .map(|m| Hit {
                    passage: m.psg,
                    position: None,
                    bytes: None,
                    epub: Some(EpubPlace {
                        spine_index: m.location.spine_index,
                        chapter: m.location.chapter,
                        text_utf16: m.location.text_utf16,
                        line: m.location.line,
                        utf16_column: m.location.utf16_column,
                        line_text: m.location.line_text,
                    }),
                    label: Some(m.location.cfi),
                })
                .collect();
            Ok(Some(FileHits {
                path: path_buf,
                text: None,
                hits,
            }))
        }
        #[cfg(feature = "pdf")]
        Some("pdf") => {
            let doc = mupdf::Document::open(path).map_err(|e| e.to_string())?;
            use topos_bible_formats::SearchFormat as _;
            let matches = matcher
                .search_format::<topos_bible_formats::pdf::PDFLocation>(&doc)
                .map_err(|e| e.to_string())?;
            let hits = matches
                .into_iter()
                .map(|m| Hit {
                    passage: m.psg,
                    position: None,
                    bytes: None,
                    label: Some(format!("page {}", m.location.page)),
                    epub: None,
                })
                .collect();
            Ok(Some(FileHits {
                path: path_buf,
                text: None,
                hits,
            }))
        }
        _ => {
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            // Like ripgrep, skip files that look binary
            if !binary && bytes[..bytes.len().min(8192)].contains(&0) {
                return Ok(None);
            }
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let mut found = search_text(matcher, path_buf, text);
            if matches!(extension.as_deref(), Some("srt" | "vtt" | "sbv")) {
                label_timestamps(&mut found);
            }
            Ok(Some(found))
        }
    }
}

/// Searches text (from a file at `path`, or given directly)
pub fn search_text(matcher: &BibleMatcher, path: Option<PathBuf>, text: String) -> FileHits {
    let hits = matcher
        .search(&text)
        .into_iter()
        .map(|m| Hit {
            passage: m.psg,
            position: Some((m.location.start, m.location.end)),
            bytes: Some(m.location.bytes.start..m.location.bytes.end),
            label: None,
            epub: None,
        })
        .collect();
    FileHits {
        path,
        text: Some(Arc::from(text)),
        hits,
    }
}

/// Labels subtitle hits with the time their cue starts
fn label_timestamps(found: &mut FileHits) {
    let Some(text) = found.text.clone() else {
        return;
    };
    let doc = SRTDocument::parse(&text);
    for hit in &mut found.hits {
        let cue = hit.bytes.as_ref().and_then(|b| doc.cue_at(b.start));
        hit.label = cue.map(|cue| timestamp(cue.start));
    }
}

fn timestamp(t: SRTTimeStamp) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        t.hours, t.minutes, t.seconds, t.millis
    )
}
