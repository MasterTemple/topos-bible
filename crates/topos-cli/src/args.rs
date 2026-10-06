use std::{fs, path::PathBuf};

use crate::{complete, search::WalkOptions};

use clap::{Parser, ValueEnum, ValueHint};
use clap_complete::engine::ArgValueCompleter;
use topos_bible::{
    data::{
        bible_data::{BibleData, BibleDataInput},
        patch::DataPatch,
    },
    error::AnyResult,
    filter::{
        bible_filter::BibleFilter,
        filters::{book::BookFilter, genre::GenreFilter, testament::TestamentFilter},
    },
    matcher::{BibleMatcher, context::BookContext},
    segments::formatter::{BookStyle, FormatOptions},
};

/**
Find Bible references in files, directories, text, or stdin.

- Including a testament limits the search to it: `--nt -g Gospels` is the four Gospels
- Included genres and books add up: `-g Pentateuch -b Revelation` is six books
- Exclusions always win, so a book can be excluded from an included genre
- Passage filters that keep references (`-i`, `--any-overlap`, `-o`, `--exact-overlap`) are joined
  with a logical OR, then `--exclude-overlap` removes references
*/
#[derive(Parser, Debug)]
#[command(
    name = "topos",
    version,
    about = "Find Bible references, like ripgrep",
    verbatim_doc_comment,
    // Options from the config file come first, so the command line overrides them
    args_override_self = true
)]
pub struct Args {
    /// Files or directories to search (respecting .gitignore); defaults to stdin when piped,
    /// otherwise the current directory
    #[arg(value_hint = ValueHint::AnyPath)]
    pub paths: Vec<PathBuf>,

    /// Search this text instead of files
    #[arg(long, conflicts_with = "paths")]
    pub text: Option<String>,

    /// Include books from a testament (old/new)
    #[arg(long = "testament", short = 't', add = ArgValueCompleter::new(complete::testaments))]
    pub testaments: Vec<TestamentFilter>,

    /// Include the New Testament (same as `-t new`)
    #[arg(long)]
    pub nt: bool,

    /// Include the Old Testament (same as `-t old`)
    #[arg(long)]
    pub ot: bool,

    /// Exclude books from a testament
    #[arg(long = "exclude-testament", add = ArgValueCompleter::new(complete::testaments))]
    pub exclude_testaments: Vec<TestamentFilter>,

    /// Include books of a genre (e.g. epistles, gospels)
    #[arg(long = "genre", short = 'g', add = ArgValueCompleter::new(complete::genres))]
    pub genres: Vec<String>,

    /// Exclude books of a genre
    #[arg(long = "exclude-genre", add = ArgValueCompleter::new(complete::genres))]
    pub exclude_genres: Vec<String>,

    /// Include a book (e.g. John)
    #[arg(long = "book", short = 'b', add = ArgValueCompleter::new(complete::books))]
    pub books: Vec<String>,

    /// Exclude a book
    #[arg(long = "exclude-book", add = ArgValueCompleter::new(complete::books))]
    pub exclude_books: Vec<String>,

    /// Only keep references entirely inside this passage (e.g. "John 1" keeps John 1:2-3)
    #[arg(long = "inside", short = 'i', add = ArgValueCompleter::new(complete::passages))]
    pub inside: Vec<String>,

    /// Only keep references that share any verse with this passage, whole chapters included
    /// (e.g. "John 3:16" keeps John 3 and John 3:14-18)
    #[arg(
        long = "any-overlap",
        alias = "overlaps",
        add = ArgValueCompleter::new(complete::passages)
    )]
    pub any_overlap: Vec<String>,

    /// Only keep references that name a verse of this passage: whole chapters don't count
    /// (e.g. "John 3:16" keeps John 3:14-18 and John 2; 3:16, but not John 3)
    #[arg(
        long = "explicit-overlap",
        short = 'o',
        add = ArgValueCompleter::new(complete::passages)
    )]
    pub explicit_overlap: Vec<String>,

    /// Only keep references that are exactly this passage, however they are written (e.g.
    /// "John 3:16-18" keeps Jn 3:16-18 and John 3:16, 17-18, but not John 3:16-17)
    #[arg(long = "exact-overlap", add = ArgValueCompleter::new(complete::passages))]
    pub exact_overlap: Vec<String>,

    /// Drop references that share any verse with this passage
    #[arg(
        long = "exclude-overlap",
        alias = "outside",
        add = ArgValueCompleter::new(complete::passages)
    )]
    pub exclude_overlap: Vec<String>,

    /// Treat the input as being about this book, so references like 3:16 match
    #[arg(long, add = ArgValueCompleter::new(complete::books))]
    pub context_book: Option<String>,

    /// Lines matching this pattern set the book for references after them, like '^#+ {book}$'
    #[arg(long, conflicts_with = "context_book")]
    pub context_heading: Option<String>,

    /// A JSON file with custom books, genres, or chapter and verse counts
    #[arg(long, value_hint = ValueHint::FilePath)]
    pub data: Option<PathBuf>,

    /// A JSON file (like --data, every field optional) whose names and values are added to the
    /// data, keeping the defaults: new abbreviations, books, genres, or chapter counts
    #[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath)]
    pub merge_data: Vec<PathBuf>,

    /// A JSON file (like --data, every field optional) whose values are removed from the data: a
    /// book or genre listed alone is removed entirely, otherwise just the values listed
    #[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath)]
    pub remove_data: Vec<PathBuf>,

    /// Use a named query from ~/.config/topos/queries.toml (its options go where this is)
    #[arg(long, short = 'q', value_name = "NAME", add = ArgValueCompleter::new(complete::queries))]
    pub query: Vec<String>,

    /// List the named queries and exit
    #[arg(long)]
    pub list_queries: bool,

    /// Read default options from this file instead of ~/.config/topos/config.toml
    #[arg(long, value_name = "PATH", conflicts_with = "no_config", value_hint = ValueHint::FilePath)]
    pub config: Option<PathBuf>,

    /// Do not read default options from a config file
    #[arg(long)]
    pub no_config: bool,

    /// Print only the total number of matches across all files (same as `-m total-count`)
    #[arg(long)]
    pub total_count: bool,

    /// How to print results
    #[arg(long, short = 'm', value_enum, default_value_t)]
    pub mode: OutputMode,

    /// How to write each reference
    #[arg(long, short = 'f', value_enum, default_value_t)]
    pub format: ReferenceFormat,

    /// Lines of context to show after each match
    #[arg(long, short = 'A', default_value_t = 0)]
    pub after_context: usize,

    /// Lines of context to show before each match
    #[arg(long, short = 'B', default_value_t = 0)]
    pub before_context: usize,

    /// Lines of context to show before and after each match
    #[arg(long, short = 'C')]
    pub context: Option<usize>,

    /// When to use colors
    #[arg(long, value_enum, default_value_t)]
    pub color: ColorChoice,

    /// Print results sorted by path (waits for the whole search)
    #[arg(long)]
    pub sort: bool,

    /// Reuse results for files that have not changed (stored unfiltered, so any filters can use them)
    #[arg(long, overrides_with = "no_cache")]
    pub cache: bool,

    /// Don't use the cache, even if the config turns it on
    #[arg(long, overrides_with = "cache")]
    pub no_cache: bool,

    /// Print the completions for a partly typed reference, one per line, and exit: books for ""
    /// (or nothing), then chapters, verses, and range ends ("John 3:" gives John 3:1, ...).
    /// Uses -f for the book style and the book filters; -m json prints objects
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        default_missing_value = "",
        add = ArgValueCompleter::new(complete::passages)
    )]
    pub complete: Option<String>,

    /// Print every book, one per line, and exit (the same as `--complete ""`)
    #[arg(long)]
    pub list_books: bool,

    /// Delete the cache (in ~/.cache/topos) and exit
    #[arg(long)]
    pub clear_cache: bool,

    /// Only search files with these extensions when walking directories (e.g. md,txt); files
    /// named on the command line are always searched
    #[arg(long = "ext", value_name = "EXT", value_delimiter = ',')]
    pub extensions: Vec<String>,

    /// Don't search files with these extensions when walking directories (e.g. pdf,epub)
    #[arg(long, value_name = "EXT", value_delimiter = ',')]
    pub exclude_ext: Vec<String>,

    /// Include or exclude paths matching a glob (`!` excludes), like `--glob '*.md' --glob
    /// '!drafts/**'`; can be repeated. Like ripgrep, globs override ignore files and --hidden
    #[arg(long, value_name = "GLOB")]
    pub glob: Vec<String>,

    /// Like --glob, ignoring case (when a path matches both, the --iglob wins)
    #[arg(long, value_name = "GLOB")]
    pub iglob: Vec<String>,

    /// Search hidden files and directories (names starting with `.`)
    #[arg(long, short = '.')]
    pub hidden: bool,

    /// Don't respect ignore files (.gitignore, .ignore, .toposignore, and Git's global and
    /// exclude files)
    #[arg(long)]
    pub no_ignore: bool,

    /// Don't respect Git's ignore files (.gitignore, the global gitignore, .git/info/exclude)
    #[arg(long)]
    pub no_ignore_vcs: bool,

    /// Don't respect ignore files in parent directories
    #[arg(long)]
    pub no_ignore_parent: bool,

    /// Respect .gitignore files outside of Git repositories too
    #[arg(long)]
    pub no_require_git: bool,

    /// Also ignore the paths in this file (gitignore syntax); can be repeated
    #[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath)]
    pub ignore_file: Vec<PathBuf>,

    /// Search more: -u is --no-ignore, -uu adds --hidden, -uuu adds --binary
    #[arg(long, short = 'u', action = clap::ArgAction::Count)]
    pub unrestricted: u8,

    /// Search files that look binary as text (normally they are skipped)
    #[arg(long)]
    pub binary: bool,

    /// Follow symbolic links
    #[arg(long, short = 'L')]
    pub follow: bool,

    /// Descend at most this many directories below the paths given (0 searches only them)
    #[arg(long, short = 'd', value_name = "NUM")]
    pub max_depth: Option<usize>,

    /// Skip files larger than this, like 500K, 10M, or 1G
    #[arg(long, value_name = "SIZE", value_parser = parse_size)]
    pub max_filesize: Option<u64>,

    /// Don't cross into other file systems (like mounted drives)
    #[arg(long)]
    pub one_file_system: bool,

    /// Print the files that would be searched, without searching them
    #[arg(long)]
    pub files: bool,

    /// Print only the paths of files with references
    #[arg(long, short = 'l', conflicts_with = "files_without_match")]
    pub files_with_matches: bool,

    /// Print only the paths of searched files without references
    #[arg(long)]
    pub files_without_match: bool,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputMode {
    /// Grouped by file on a terminal, otherwise `path:line:column: reference`
    #[default]
    Auto,
    /// Grouped by file, with optional context lines
    Grouped,
    /// `path:line:column: reference` (for Vim's quickfix list)
    #[value(alias = "qf", alias = "vimgrep")]
    Quickfix,
    /// A Markdown table
    #[value(alias = "t")]
    Table,
    /// One JSON object per match
    #[value(alias = "j")]
    Json,
    /// Matches per file
    #[value(alias = "c")]
    Count,
    /// Matches across all files
    TotalCount,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ReferenceFormat {
    /// `Genesis 1:1`
    #[default]
    Name,
    /// `Gn 1:1`
    #[value(alias = "abbrev")]
    Abbreviation,
    /// `Gen.1.1`
    Osis,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

/// `500`, `500K`, `10M`, or `1G` (powers of 1024)
fn parse_size(text: &str) -> Result<u64, String> {
    let text = text.trim();
    let invalid = || format!("`{text}` isn't a size like 500K, 10M, or 1G");
    let split = text
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let number: u64 = number.trim().parse().map_err(|_| invalid())?;
    let scale: u64 = match unit.to_ascii_uppercase().as_str() {
        "" | "B" => 1,
        "K" | "KB" => 1 << 10,
        "M" | "MB" => 1 << 20,
        "G" | "GB" => 1 << 30,
        _ => return Err(invalid()),
    };
    Ok(number.saturating_mul(scale))
}

impl Args {
    /// How directories are walked, with -u applied
    pub fn walk_options(&self) -> Result<WalkOptions, String> {
        let u = self.unrestricted;
        let mut overrides = None;
        if !self.glob.is_empty() || !self.iglob.is_empty() {
            let root = std::env::current_dir().map_err(|e| e.to_string())?;
            let mut builder = ignore::overrides::OverrideBuilder::new(root);
            for glob in &self.glob {
                builder
                    .add(glob)
                    .map_err(|e| format!("--glob {glob}: {e}"))?;
            }
            builder.case_insensitive(true).map_err(|e| e.to_string())?;
            for glob in &self.iglob {
                builder
                    .add(glob)
                    .map_err(|e| format!("--iglob {glob}: {e}"))?;
            }
            overrides = Some(builder.build().map_err(|e| e.to_string())?);
        }
        let lower = |list: &[String]| -> Vec<String> {
            list.iter()
                .map(|ext| ext.trim().trim_start_matches('.').to_ascii_lowercase())
                .filter(|ext| !ext.is_empty())
                .collect()
        };
        Ok(WalkOptions {
            hidden: self.hidden || u >= 2,
            no_ignore: self.no_ignore || u >= 1,
            no_ignore_vcs: self.no_ignore_vcs,
            no_ignore_parent: self.no_ignore_parent,
            no_require_git: self.no_require_git,
            ignore_files: self.ignore_file.clone(),
            follow: self.follow,
            max_depth: self.max_depth,
            max_filesize: self.max_filesize,
            one_file_system: self.one_file_system,
            overrides,
            extensions: lower(&self.extensions),
            exclude_extensions: lower(&self.exclude_ext),
        })
    }

    /// Whether files that look binary are searched (`--binary` or `-uuu`)
    pub fn search_binary(&self) -> bool {
        self.binary || self.unrestricted >= 3
    }

    /// Every option that changes which references are found before filtering (for the cache,
    /// which stores unfiltered results, so filters are left out)
    pub fn fingerprint(&self) -> String {
        let contents = |paths: &[PathBuf]| -> Vec<Option<String>> {
            paths
                .iter()
                .map(|path| fs::read_to_string(path).ok())
                .collect()
        };
        let data = contents(self.data.as_slice());
        let merge = contents(&self.merge_data);
        let remove = contents(&self.remove_data);
        // Binary files are cached as having no references unless they are searched
        let binary = self.search_binary();
        format!(
            "{:?}",
            (
                &self.context_book,
                &self.context_heading,
                data,
                merge,
                remove,
                binary
            )
        )
    }

    pub fn context_lines(&self) -> (usize, usize) {
        match self.context {
            Some(lines) => (
                lines.max(self.before_context),
                lines.max(self.after_context),
            ),
            None => (self.before_context, self.after_context),
        }
    }

    /// Book style for the core formatter (OSIS is written by [`Passage::to_osis`] instead)
    pub fn format_options(&self) -> FormatOptions {
        FormatOptions {
            book: match self.format {
                ReferenceFormat::Name => BookStyle::Name,
                ReferenceFormat::Abbreviation => BookStyle::Abbreviation,
                ReferenceFormat::Osis => BookStyle::Osis,
            },
            ..FormatOptions::default()
        }
    }

    /// `--data` (or the defaults), then each `--merge-data`, then each `--remove-data`
    fn data(&self) -> AnyResult<BibleData> {
        fn read<T: serde::de::DeserializeOwned>(path: &PathBuf) -> AnyResult<T> {
            let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?)
        }
        if self.data.is_none() && self.merge_data.is_empty() && self.remove_data.is_empty() {
            return Ok(BibleData::default());
        }
        let mut input = match &self.data {
            Some(path) => read::<BibleDataInput>(path)?,
            None => BibleDataInput::defaults(),
        };
        for path in &self.merge_data {
            input
                .merge(read::<DataPatch>(path)?)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        for path in &self.remove_data {
            input
                .remove(read::<DataPatch>(path)?)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(BibleData::new(input)?)
    }

    pub fn matcher(&self) -> AnyResult<BibleMatcher> {
        let data = self.data()?;
        let mut filter = BibleFilter::new(data);
        filter.include_many(self.testaments.iter().copied())?;
        if self.nt {
            filter.include(TestamentFilter::New)?;
        }
        if self.ot {
            filter.include(TestamentFilter::Old)?;
        }
        filter.include_many(self.genres.iter().map(GenreFilter::new))?;
        filter.include_many(self.books.iter().map(BookFilter::new))?;
        filter.exclude_many(self.exclude_testaments.iter().copied())?;
        filter.exclude_many(self.exclude_genres.iter().map(GenreFilter::new))?;
        filter.exclude_many(self.exclude_books.iter().map(BookFilter::new))?;
        for passage in &self.inside {
            filter.filter_inside(passage)?;
        }
        for passage in &self.any_overlap {
            filter.filter_any_overlap(passage)?;
        }
        for passage in &self.explicit_overlap {
            filter.filter_explicit_overlap(passage)?;
        }
        for passage in &self.exact_overlap {
            filter.filter_exact_overlap(passage)?;
        }
        for passage in &self.exclude_overlap {
            filter.filter_exclude_overlap(passage)?;
        }

        if let Some(reason) = filter.contradiction() {
            eprintln!("topos: warning: {reason}, so nothing can match");
        }
        let matcher = filter.create_matcher();
        let books = matcher.data().books();
        let context = match (&self.context_book, &self.context_heading) {
            (Some(book), _) => Some(BookContext::Book(
                books
                    .search(book)
                    .ok_or_else(|| format!("unknown book {book:?}"))?,
            )),
            (None, Some(pattern)) => Some(BookContext::headings(books, pattern)?),
            (None, None) => None,
        };
        Ok(match context {
            Some(context) => matcher.with_context(context),
            None => matcher,
        })
    }
}
