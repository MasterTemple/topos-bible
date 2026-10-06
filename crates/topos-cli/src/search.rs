use std::{
    fs,
    io::{self, Read},
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread,
};

use crate::cache::{Cache, Cached};
use ignore::{WalkBuilder, WalkState};
use std::collections::BTreeSet;
use topos_bible::{
    data::books::BookId,
    matcher::{BibleMatcher, Position},
    segments::Passage,
};
use topos_bible_formats::{
    SearchFormat,
    epub::CfiLocation,
    srt::{SRTDocument, SRTTimeStamp},
};

/// What to search
pub enum Input {
    Paths(Vec<PathBuf>),
    Text(String),
}

impl Input {
    /// Paths given on the command line, else `--text`, else stdin when piped, else `.`
    pub fn new(paths: Vec<PathBuf>, text: Option<String>) -> io::Result<Self> {
        if let Some(text) = text {
            return Ok(Self::Text(text));
        }
        if !paths.is_empty() {
            return Ok(Self::Paths(paths));
        }
        if stdin_is_readable() {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text)?;
            return Ok(Self::Text(text));
        }
        Ok(Self::Paths(vec![PathBuf::from(".")]))
    }
}

/// Like ripgrep, only read stdin when it is a pipe or a file (not a terminal or `/dev/null`)
fn stdin_is_readable() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        fs::metadata("/dev/stdin").is_ok_and(|m| m.is_file() || m.file_type().is_fifo())
    }
    #[cfg(not(unix))]
    {
        use std::io::IsTerminal;
        !io::stdin().is_terminal()
    }
}

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

/// How files are searched
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
}

/// How directories are walked (the path options)
#[derive(Default)]
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
    pub overrides: Option<ignore::overrides::Override>,
    /// Lowercase extensions to search (empty searches every file)
    pub extensions: Vec<String>,
    /// Lowercase extensions to skip
    pub exclude_extensions: Vec<String>,
}

impl WalkOptions {
    /// A walker over `paths` with these options, like ripgrep's
    pub fn builder(&self, paths: &[PathBuf]) -> Result<WalkBuilder, String> {
        let mut builder = WalkBuilder::new(&paths[0]);
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
        if let Some(overrides) = &self.overrides {
            builder.overrides(overrides.clone());
        }
        Ok(builder)
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

/// Searches the input, sending each file's result as soon as it is ready
pub fn search(searcher: Arc<Searcher>, input: Input) -> mpsc::Receiver<FileResult> {
    let (sender, receiver) = mpsc::channel();
    match input {
        Input::Text(text) => {
            let _ = sender.send(Ok(search_text(&searcher.matcher, None, text)));
        }
        Input::Paths(paths) => {
            thread::spawn(move || walk(searcher, paths, sender));
        }
    }
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
                Err(err) => Err((None, err.to_string())),
            };
            match sender.send(result) {
                Ok(()) => WalkState::Continue,
                Err(_) => WalkState::Quit,
            }
        })
    });
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
            return search_file(&self.matcher, path, self.binary);
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
                let found = search_file(&cached.unfiltered, path, self.binary)?;
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

/// `Ok(None)` for files that are skipped (binary files without a supported format)
fn search_file(
    matcher: &BibleMatcher,
    path: &Path,
    binary: bool,
) -> Result<Option<FileHits>, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let path_buf = Some(path.to_path_buf());
    match extension.as_deref() {
        Some("epub") => {
            let matches = matcher
                .search_format::<CfiLocation>(path)
                .map_err(|e| e.to_string())?;
            let hits = matches
                .into_iter()
                .map(|m| Hit {
                    passage: m.psg,
                    position: None,
                    bytes: None,
                    label: Some(m.location.start_cfi),
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

fn search_text(matcher: &BibleMatcher, path: Option<PathBuf>, text: String) -> FileHits {
    let hits = matcher
        .search(&text)
        .into_iter()
        .map(|m| Hit {
            passage: m.psg,
            position: Some((m.location.start, m.location.end)),
            bytes: Some(m.location.bytes.start..m.location.bytes.end),
            label: None,
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
