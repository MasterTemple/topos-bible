use std::{fs, path::PathBuf};

use crate::complete;

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
- `--inside` and `--overlaps` passages are joined with a logical OR, then `--outside` removes matches
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

    /// Only keep references that share any verse with this passage (e.g. "John 1" keeps
    /// John 1:51-2:1)
    #[arg(long = "overlaps", short = 'o', add = ArgValueCompleter::new(complete::passages))]
    pub overlaps: Vec<String>,

    /// Drop references that share any verse with this passage
    #[arg(long = "outside", add = ArgValueCompleter::new(complete::passages))]
    pub outside: Vec<String>,

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

impl Args {
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
        format!(
            "{:?}",
            (
                &self.context_book,
                &self.context_heading,
                data,
                merge,
                remove
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
        for passage in &self.overlaps {
            filter.filter_overlaps(passage)?;
        }
        for passage in &self.outside {
            filter.filter_outside(passage)?;
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
