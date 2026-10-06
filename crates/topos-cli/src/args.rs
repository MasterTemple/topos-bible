use std::{fs, path::PathBuf};

use clap::{Parser, ValueEnum};
use topos_lib::{
    data::bible_data::{BibleData, BibleDataInput},
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

- Including a testament, genre, or book excludes everything else in that category
- Exclusions are applied after inclusions, so a book can be excluded from an included genre
- Several inclusions of the same kind are joined with a logical OR
*/
#[derive(Parser, Debug)]
#[command(
    name = "topos",
    version,
    about = "Find Bible references, like ripgrep",
    verbatim_doc_comment
)]
pub struct Args {
    /// Files or directories to search (respecting .gitignore); defaults to stdin when piped,
    /// otherwise the current directory
    pub paths: Vec<PathBuf>,

    /// Search this text instead of files
    #[arg(long, conflicts_with = "paths")]
    pub text: Option<String>,

    /// Include books from a testament (old/new)
    #[arg(long = "testament", short = 't')]
    pub testaments: Vec<TestamentFilter>,

    /// Exclude books from a testament
    #[arg(long = "exclude-testament")]
    pub exclude_testaments: Vec<TestamentFilter>,

    /// Include books of a genre (e.g. epistles, gospels)
    #[arg(long = "genre", short = 'g')]
    pub genres: Vec<String>,

    /// Exclude books of a genre
    #[arg(long = "exclude-genre")]
    pub exclude_genres: Vec<String>,

    /// Include a book (e.g. John)
    #[arg(long = "book", short = 'b')]
    pub books: Vec<String>,

    /// Exclude a book
    #[arg(long = "exclude-book")]
    pub exclude_books: Vec<String>,

    /// Only keep references that overlap this passage (e.g. "John 1:2-3")
    #[arg(long = "inside", short = 'i')]
    pub inside: Vec<String>,

    /// Drop references that overlap this passage (e.g. "John 3:4-5")
    #[arg(long = "outside", short = 'o')]
    pub outside: Vec<String>,

    /// Treat the input as being about this book, so references like 3:16 match
    #[arg(long)]
    pub context_book: Option<String>,

    /// Lines matching this pattern set the book for references after them, like '^#+ {book}$'
    #[arg(long, conflicts_with = "context_book")]
    pub context_heading: Option<String>,

    /// A JSON file with custom books, genres, or chapter and verse counts
    #[arg(long)]
    pub config: Option<PathBuf>,

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

    pub fn matcher(&self) -> AnyResult<BibleMatcher> {
        let data = match &self.config {
            Some(path) => {
                let input: BibleDataInput = serde_json::from_str(&fs::read_to_string(path)?)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                BibleData::new(input)?
            }
            None => BibleData::default(),
        };
        let mut filter = BibleFilter::new(data);
        filter.include_many(self.testaments.iter().copied())?;
        filter.include_many(self.genres.iter().map(GenreFilter::new))?;
        filter.include_many(self.books.iter().map(BookFilter::new))?;
        filter.exclude_many(self.exclude_testaments.iter().copied())?;
        filter.exclude_many(self.exclude_genres.iter().map(GenreFilter::new))?;
        filter.exclude_many(self.exclude_books.iter().map(BookFilter::new))?;
        for passage in &self.inside {
            filter.filter_inside(passage)?;
        }
        for passage in &self.outside {
            filter.filter_outside(passage)?;
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
