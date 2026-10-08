/*!
Searching files and folders, the way the `topos` CLI does (feature `files`; not in WebAssembly)

- [`WalkOptions`]: which files are searched, like ripgrep (ignore files, hidden files, globs,
  extensions, depth)
- [`search`]: walks in parallel and searches each file by its format: EPUBs (with CFIs), PDFs
  (feature `pdf`), subtitles (labeled with their times), and anything else as text (skipping
  files that look binary)
- [`Cache`]: each file's unfiltered results, reused while the file is unchanged, in a folder of
  your choice; [`CachedSearch`] filters them for each search
*/

mod cache;
mod search;

pub use cache::{Cache, Cached, clear, default_root};
pub use search::{
    CachedSearch, EpubPlace, FileHits, FileResult, Hit, Searcher, WalkOptions, is_epub, search,
    search_file, search_text,
};
pub use topos_bible_formats::epub::CfiOptions;

/// What, besides the matcher's data, changes which references are found before filtering: a
/// cache is kept per fingerprint (`config` describes the matcher's data and book context)
pub fn fingerprint(config: &str, binary: bool, cfi: CfiOptions) -> String {
    format!("{config}\0binary={binary}\0assertions={}", cfi.assertions)
}
