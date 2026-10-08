//! Searching files and folders like the `topos` CLI: the same walking options, formats, and
//! cache (not in WebAssembly, which has no file access)

use boltffi::*;

use crate::{OffsetUnit, Passage, Topos, ToposError, ToposQuery};

/**
Which files are searched and how, like the CLI's options. Chain the methods (each returns new
options)

```ts
const files = ToposFiles.create().extension("md").glob("!Archive").cacheDir(cacheFolder);
topos.searchFiles(["notes"], files, ToposQuery.create(), OffsetUnit.Utf16);
```

- Like ripgrep, `.gitignore`, `.ignore`, and `.toposignore` files are respected and hidden files
  are skipped, unless told otherwise; files named directly are always searched
- With a cache, each file's results are kept (unfiltered) until the file changes
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToposFiles {
    pub hidden: bool,
    pub no_ignore: bool,
    pub no_ignore_vcs: bool,
    pub no_ignore_parent: bool,
    pub no_require_git: bool,
    pub ignore_files: Vec<String>,
    pub follow_links: bool,
    pub max_depth: Option<u32>,
    pub max_filesize: Option<u64>,
    pub one_file_system: bool,
    pub globs: Vec<String>,
    pub iglobs: Vec<String>,
    pub extensions: Vec<String>,
    pub exclude_extensions: Vec<String>,
    pub binary: bool,
    pub cfi_assertions: bool,
    pub cache: bool,
    pub cache_dir: Option<String>,
}

fn with<T>(mut list: Vec<T>, value: T) -> Vec<T> {
    list.push(value);
    list
}

#[export]
impl ToposFiles {
    /// The CLI's defaults
    pub fn create() -> Self {
        Self::default()
    }

    /// Search hidden files and folders (`--hidden`)
    pub fn hidden(&self, yes: bool) -> Self {
        Self {
            hidden: yes,
            ..self.clone()
        }
    }

    /// Don't respect ignore files (`--no-ignore`)
    pub fn no_ignore(&self, yes: bool) -> Self {
        Self {
            no_ignore: yes,
            ..self.clone()
        }
    }

    /// Don't respect `.gitignore` and other version control ignore files (`--no-ignore-vcs`)
    pub fn no_ignore_vcs(&self, yes: bool) -> Self {
        Self {
            no_ignore_vcs: yes,
            ..self.clone()
        }
    }

    /// Don't respect ignore files in parent folders (`--no-ignore-parent`)
    pub fn no_ignore_parent(&self, yes: bool) -> Self {
        Self {
            no_ignore_parent: yes,
            ..self.clone()
        }
    }

    /// Respect `.gitignore` outside git repositories too (`--no-require-git`)
    pub fn no_require_git(&self, yes: bool) -> Self {
        Self {
            no_require_git: yes,
            ..self.clone()
        }
    }

    /// Another ignore file, in `.gitignore`'s format (`--ignore-file`)
    pub fn ignore_file(&self, path: String) -> Self {
        Self {
            ignore_files: with(self.ignore_files.clone(), path),
            ..self.clone()
        }
    }

    /// Follow symbolic links (`--follow`)
    pub fn follow_links(&self, yes: bool) -> Self {
        Self {
            follow_links: yes,
            ..self.clone()
        }
    }

    /// How deep to go into folders (`--max-depth`; 0 is only the paths given)
    pub fn max_depth(&self, depth: u32) -> Self {
        Self {
            max_depth: Some(depth),
            ..self.clone()
        }
    }

    /// Skip files bigger than this many bytes (`--max-filesize`)
    pub fn max_filesize(&self, bytes: u64) -> Self {
        Self {
            max_filesize: Some(bytes),
            ..self.clone()
        }
    }

    /// Don't cross into other file systems (`--one-file-system`)
    pub fn one_file_system(&self, yes: bool) -> Self {
        Self {
            one_file_system: yes,
            ..self.clone()
        }
    }

    /// Only files matching a glob, or not with `!` (`--glob`), relative to the current folder
    pub fn glob(&self, glob: String) -> Self {
        Self {
            globs: with(self.globs.clone(), glob),
            ..self.clone()
        }
    }

    /// A glob matched without regard to case (`--iglob`)
    pub fn iglob(&self, glob: String) -> Self {
        Self {
            iglobs: with(self.iglobs.clone(), glob),
            ..self.clone()
        }
    }

    /// Only files with this extension (`--ext`; any of those given)
    pub fn extension(&self, extension: String) -> Self {
        Self {
            extensions: with(self.extensions.clone(), extension),
            ..self.clone()
        }
    }

    /// Skip files with this extension (`--exclude-ext`)
    pub fn exclude_extension(&self, extension: String) -> Self {
        Self {
            exclude_extensions: with(self.exclude_extensions.clone(), extension),
            ..self.clone()
        }
    }

    /// Search files that look binary as text (`--binary`)
    pub fn binary(&self, yes: bool) -> Self {
        Self {
            binary: yes,
            ..self.clone()
        }
    }

    /// Write `[id]` assertions in EPUB CFIs (`--cfi-assertions`)
    pub fn cfi_assertions(&self, yes: bool) -> Self {
        Self {
            cfi_assertions: yes,
            ..self.clone()
        }
    }

    /// Keep each file's results until it changes (`--cache`), in the CLI's cache folder
    /// (`~/.cache/topos`) unless `cache_dir` names another
    pub fn cache(&self, yes: bool) -> Self {
        Self {
            cache: yes,
            ..self.clone()
        }
    }

    /// Keep the cache in this folder (an app's cache folder, on a phone); turns the cache on
    pub fn cache_dir(&self, dir: String) -> Self {
        Self {
            cache: true,
            cache_dir: Some(dir),
            ..self.clone()
        }
    }
}

/// Where a reference in an EPUB is
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEpubPlace {
    pub spine_index: u32,
    pub cfi: String,
    /// The table of contents' name for its content document
    pub chapter: Option<String>,
    /// The line (paragraph) it starts in
    pub line_text: String,
}

/// A reference in a file
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMatch {
    pub passage: Passage,
    /// Start and end offsets in the requested unit (for EPUBs, UTF-16 offsets through the book's
    /// text: each content document's, a blank line apart); none for PDFs
    pub start: Option<u32>,
    pub end: Option<u32>,
    /// 1-based line and column of the start (the column in the requested unit)
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// Where it is in the format's own terms: a subtitle's time, a PDF's page, an EPUB's CFI
    pub label: Option<String>,
    pub epub: Option<FileEpubPlace>,
}

/// One file's references, or why it couldn't be searched
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMatches {
    /// As found from the paths given (empty for an error that isn't about one file)
    pub path: String,
    pub matches: Vec<FileMatch>,
    pub error: Option<String>,
}

#[export]
impl Topos {
    /**
    Searches files and folders like the CLI: every file the options allow, by its format (EPUBs
    with CFIs, subtitles with times, anything else as text), keeping the references the query
    keeps. Files come back sorted by path, each with its references or an error
    */
    pub fn search_files(
        &self,
        paths: Vec<String>,
        files: &ToposFiles,
        query: &ToposQuery,
        unit: OffsetUnit,
    ) -> Result<Vec<FileMatches>, ToposError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            native::search(self, paths, files, query, unit, false)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (paths, files, query, unit);
            Err(no_files())
        }
    }

    /// The files `search_files` would search, sorted, without searching them (`--files`)
    pub fn list_files(
        &self,
        paths: Vec<String>,
        files: &ToposFiles,
    ) -> Result<Vec<String>, ToposError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let query = ToposQuery::default();
            let found = native::search(self, paths, files, &query, OffsetUnit::Byte, true)?;
            Ok(found.into_iter().map(|f| f.path).collect())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (paths, files);
            Err(no_files())
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn no_files() -> ToposError {
    ToposError::Files {
        message: "WebAssembly has no file access: read files yourself and use search or ToposIndex"
            .into(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::{path::PathBuf, sync::Arc};

    use topos_bible_index::files::{
        self, Cache, CachedSearch, CfiOptions, FileHits, Searcher, WalkOptions,
    };

    use super::{FileEpubPlace, FileMatch, FileMatches, ToposFiles};
    use crate::{OffsetUnit, Topos, ToposError, ToposQuery};

    pub fn search(
        topos: &Topos,
        paths: Vec<String>,
        files: &ToposFiles,
        query: &ToposQuery,
        unit: OffsetUnit,
        list_only: bool,
    ) -> Result<Vec<FileMatches>, ToposError> {
        let matcher = topos.filter(query)?.create_matcher();
        let matcher = match topos.matcher.context() {
            Some(context) => matcher.with_context(context.clone()),
            None => matcher,
        };
        let walk = WalkOptions {
            hidden: files.hidden,
            no_ignore: files.no_ignore,
            no_ignore_vcs: files.no_ignore_vcs,
            no_ignore_parent: files.no_ignore_parent,
            no_require_git: files.no_require_git,
            ignore_files: files.ignore_files.iter().map(PathBuf::from).collect(),
            follow: files.follow_links,
            max_depth: files.max_depth.map(|d| d as usize),
            max_filesize: files.max_filesize,
            one_file_system: files.one_file_system,
            globs: files.globs.clone(),
            iglobs: files.iglobs.clone(),
            extensions: WalkOptions::extension_list(&files.extensions),
            exclude_extensions: WalkOptions::extension_list(&files.exclude_extensions),
            only_epub: false,
        };
        let paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        // A bad glob or ignore file is the caller's mistake, not one file's
        walk.builder(&paths)
            .map_err(|message| ToposError::Files { message })?;
        let cfi = CfiOptions {
            assertions: files.cfi_assertions,
        };
        let cached = (files.cache && !list_only)
            .then(|| {
                let fingerprint = files::fingerprint(&topos.config, files.binary, cfi);
                match &files.cache_dir {
                    Some(dir) => Cache::open_in(std::path::Path::new(dir), &fingerprint),
                    None => Cache::open(&fingerprint),
                }
            })
            .flatten()
            .map(|cache| CachedSearch::new(cache, &matcher));
        let searcher = Arc::new(Searcher {
            matcher,
            cached,
            // Offsets in the caller's unit are counted in the text
            needs_text: true,
            walk,
            binary: files.binary,
            list_only,
            cfi,
        });
        let mut found: Vec<FileMatches> = files::search(searcher.clone(), paths)
            .into_iter()
            .map(|result| match result {
                Ok(file) => FileMatches {
                    path: path_of(&file),
                    matches: matches(topos, &file, unit),
                    error: None,
                },
                Err((path, error)) => FileMatches {
                    path: path.map(|p| p.display().to_string()).unwrap_or_default(),
                    matches: vec![],
                    error: Some(error),
                },
            })
            .collect();
        if let Some(cached) = &searcher.cached {
            cached.cache.finish();
        }
        found.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(found)
    }

    fn path_of(file: &FileHits) -> String {
        file.path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    }

    fn matches(topos: &Topos, file: &FileHits, unit: OffsetUnit) -> Vec<FileMatch> {
        let mut offsets = Offsets::default();
        file.hits
            .iter()
            .filter_map(|hit| {
                let passage = topos.passage(&hit.passage, &topos.format)?;
                let (mut start, mut end, mut line, mut column) = (None, None, None, None);
                if let (Some(text), Some(bytes), Some((from, _))) =
                    (&file.text, &hit.bytes, hit.position)
                {
                    let line_start = text[..bytes.start].rfind('\n').map_or(0, |i| i + 1);
                    let at = offsets.of(text, bytes.start, unit);
                    start = Some(at);
                    end = Some(offsets.of(text, bytes.end, unit));
                    line = Some(from.line as u32);
                    column = Some(count(&text[line_start..bytes.start], unit) + 1);
                }
                let epub = hit.epub.as_ref().map(|place| {
                    start = Some(place.text_utf16.start as u32);
                    end = Some(place.text_utf16.end as u32);
                    line = Some(place.line as u32);
                    column = Some(place.utf16_column as u32);
                    FileEpubPlace {
                        spine_index: place.spine_index as u32,
                        cfi: hit.label.clone().unwrap_or_default(),
                        chapter: place.chapter.clone(),
                        line_text: place.line_text.clone(),
                    }
                });
                Some(FileMatch {
                    passage,
                    start,
                    end,
                    line,
                    column,
                    label: hit.label.clone(),
                    epub,
                })
            })
            .collect()
    }

    fn count(text: &str, unit: OffsetUnit) -> u32 {
        (match unit {
            OffsetUnit::Byte => text.len(),
            OffsetUnit::Char => text.chars().count(),
            OffsetUnit::Utf16 => text.encode_utf16().count(),
        }) as u32
    }

    /// Increasing byte offsets in one text in another unit, counting each byte once
    #[derive(Default)]
    struct Offsets {
        byte: usize,
        at: u32,
    }

    impl Offsets {
        fn of(&mut self, text: &str, byte: usize, unit: OffsetUnit) -> u32 {
            if byte < self.byte {
                *self = Self::default();
            }
            self.at += count(&text[self.byte..byte], unit);
            self.byte = byte;
            self.at
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("topos-ffi-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("notes/Archive")).unwrap();
        fs::write(dir.join("notes/a.md"), "é 📖 Jn 3:16\nthen Rom 8:28").unwrap();
        fs::write(dir.join("notes/Archive/old.md"), "Gen 1:1").unwrap();
        fs::write(dir.join("notes/skip.txt"), "Ps 23").unwrap();
        fs::write(dir.join("notes/.hidden.md"), "Ps 1").unwrap();
        fs::write(
            dir.join("notes/talk.srt"),
            "1\n00:00:01,500 --> 00:00:04,000\nRead John 1:1\n",
        )
        .unwrap();
        dir
    }

    fn found(results: &[FileMatches]) -> Vec<(String, Vec<String>)> {
        results
            .iter()
            .map(|f| {
                let name = f.path.rsplit('/').next().unwrap_or_default().to_string();
                let refs = f
                    .matches
                    .iter()
                    .map(|m| m.passage.reference.clone())
                    .collect();
                (name, refs)
            })
            .collect()
    }

    #[test]
    fn searches_files_like_the_cli() {
        let dir = folder("search");
        let notes = dir.join("notes").display().to_string();
        let topos = Topos::new();
        let query = ToposQuery::default();
        let files = ToposFiles::create().exclude_extension(".TXT".into());
        let results = topos
            .search_files(vec![notes.clone()], &files, &query, OffsetUnit::Utf16)
            .unwrap();
        assert_eq!(
            found(&results),
            [
                ("old.md".into(), vec!["Genesis 1:1".into()]),
                (
                    "a.md".into(),
                    vec!["John 3:16".into(), "Romans 8:28".into()]
                ),
                ("talk.srt".into(), vec!["John 1:1".into()]),
            ]
        );
        // Offsets and columns in the unit asked for; subtitles labeled with their times
        let a = &results[1].matches;
        assert_eq!(
            (a[0].start, a[0].end, a[0].line, a[0].column),
            (Some(5), Some(12), Some(1), Some(6))
        );
        assert_eq!((a[1].line, a[1].column), (Some(2), Some(6)));
        let bytes = topos
            .search_files(vec![notes.clone()], &files, &query, OffsetUnit::Byte)
            .unwrap();
        assert_eq!(bytes[1].matches[0].start, Some(8));
        assert_eq!(results[2].matches[0].label.as_deref(), Some("00:00:01.500"));

        // Globs, hidden files, extensions, and a query
        let narrow = files
            .hidden(true)
            .extension("md".into())
            .glob("!**/Archive/**".into());
        let paul = ToposQuery::default().book("Romans".into());
        let results = topos
            .search_files(vec![notes.clone()], &narrow, &paul, OffsetUnit::Utf16)
            .unwrap();
        assert_eq!(
            found(&results),
            [
                (".hidden.md".into(), vec![]),
                ("a.md".into(), vec!["Romans 8:28".into()])
            ]
        );
        let listed = topos.list_files(vec![notes.clone()], &narrow).unwrap();
        assert_eq!(listed.len(), 2);

        // Mistakes: a bad glob is an error; a missing file is reported with its path
        assert!(matches!(
            topos.search_files(
                vec![notes.clone()],
                &files.glob("a[".into()),
                &query,
                OffsetUnit::Utf16
            ),
            Err(ToposError::Files { .. })
        ));
        let missing = dir.join("nope.md").display().to_string();
        let results = topos
            .search_files(vec![missing.clone()], &files, &query, OffsetUnit::Utf16)
            .unwrap();
        assert_eq!(results[0].path, missing);
        assert!(results[0].error.is_some());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn caches_in_a_folder_of_your_choice() {
        let dir = folder("cache");
        let notes = dir.join("notes").display().to_string();
        let cache = dir.join("cache");
        let topos = Topos::new();
        let files = ToposFiles::create().cache_dir(cache.display().to_string());
        let query = ToposQuery::default();
        let first = topos
            .search_files(vec![notes.clone()], &files, &query, OffsetUnit::Utf16)
            .unwrap();
        let entries = fs::read_dir(&cache).unwrap().count();
        assert!(entries > 0, "the cache folder has entries");
        // From the cache: the same results, and filters still apply
        let again = topos
            .search_files(vec![notes.clone()], &files, &query, OffsetUnit::Utf16)
            .unwrap();
        assert_eq!(again, first);
        let genesis = ToposQuery::default().book("Genesis".into());
        let filtered = topos
            .search_files(vec![notes], &files, &genesis, OffsetUnit::Utf16)
            .unwrap();
        let refs: Vec<_> = found(&filtered).into_iter().flat_map(|(_, r)| r).collect();
        assert_eq!(refs, ["Genesis 1:1"]);
        let _ = fs::remove_dir_all(dir);
    }
}
