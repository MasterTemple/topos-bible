use clap::Parser;
use std::path::PathBuf;
use topos_lib::{
    filter::{
        bible_filter::BibleFilter,
        filters::{book::BookFilter, genre::GenreFilter, testament::TestamentFilter},
    },
    matcher::{bible_matcher::BibleMatcher, context::BookContext},
};

use crate::outputs::OutputMode;

/**
- By positively specifying a testament/genre/book, you will implicitly telling the program to exclude the remaining items in that category.
- You may choose to exclude a subset from a larger inclusion (ex: book from a genre), however this must be specified **after** the inclusion (or else it will be re-added)
- You may combine multiple filters, and they will be joined with a logical OR
*/
#[derive(Parser, Debug)]
#[clap(
    name = "topos",
    about = "A Bible passage search tool inspired by ripgrep",
    version = "0.1.0"
)]
pub struct Args {
    #[clap(help = "The input can be a directory path, a file path, text, or stdin.")]
    pub input: Option<String>,

    // Testament filters
    #[clap(
        long = "testament",
        short = 't',
        help = "Include books from a specific testament (old/new)"
    )]
    pub testaments: Option<Vec<TestamentFilter>>,

    #[clap(
        long = "exclude-testament",
        help = "Exclude books from a specific testament"
    )]
    pub exclude_testaments: Option<Vec<TestamentFilter>>,

    // Genre filters
    #[clap(
        long = "genre",
        short = 'g',
        help = "Include books of a specific genre (e.g. epistles, gospels)"
    )]
    pub genres: Option<Vec<String>>,

    #[clap(long = "exclude-genre", help = "Exclude books of a specific genre")]
    pub exclude_genres: Option<Vec<String>>,

    // Book filters
    #[clap(
        long = "book",
        short = 'b',
        help = "Include specific books (e.g. John)"
    )]
    pub books: Option<Vec<String>>,

    #[clap(long = "exclude-book", help = "Exclude specific books")]
    pub exclude_books: Option<Vec<String>>,

    // Verse range filters
    #[clap(
        long = "inside",
        short = 'i',
        help = "Limit search to a verse range (e.g. John 1:2-3)"
    )]
    pub inside: Option<Vec<String>>,

    // Verse range filters
    #[clap(
        long = "outside",
        short = 'o',
        help = "Forbid search from matching a verse range (e.g. John 3:4-5)"
    )]
    pub outside: Option<Vec<String>>,

    #[clap(
        long = "context-book",
        help = "Treat the input as being about this book, so references like 3:16 match"
    )]
    pub context_book: Option<String>,

    #[clap(
        long = "context-heading",
        help = "Lines matching this pattern set the book for following references, like '^#+ {book}$'",
        conflicts_with = "context_book"
    )]
    pub context_heading: Option<String>,

    // TODO: actually implement this
    #[clap(long = "config", help = "Use a custom configuration file")]
    pub config: Option<PathBuf>,

    // #[clap(long = "igonre", help = "Ignore when non-real books/genres are given")]
    // pub ignore_non_existent: bool,

    // TODO: actually implement this
    #[clap(
        long = "mode",
        short = 'm',
        help = "Specify output mode",
        default_value_t
    )]
    #[arg(value_enum)]
    pub mode: OutputMode,

    #[clap(
        long = "verbose",
        short = 'v',
        help = "Include more data about each match"
    )]
    pub versbose: bool,

    // TODO: actually implement this
    #[clap(
        long = "context",
        short = 'c',
        help = "Units of context",
        default_value_t = 1
    )]
    pub context: u64,

    // TODO: actually implement this
    #[clap(
        long = "before",
        help = "Specify units of context before match to provide"
    )]
    pub before_context: Option<u64>,

    // TODO: actually implement this
    #[clap(
        long = "after",
        help = "Specify units of context after match to provide"
    )]
    pub after_context: Option<u64>,
}

impl TryFrom<Args> for BibleMatcher {
    type Error = Box<dyn std::error::Error>;

    fn try_from(args: Args) -> Result<Self, Self::Error> {
        // TODO: get alternate Bible/Genre data
        let mut filter = BibleFilter::default();

        if let Some(list) = args.testaments {
            filter.include_many(list);
        }

        if let Some(list) = args.genres {
            filter.include_many(list.into_iter().map(GenreFilter::new).collect());
        }

        if let Some(list) = args.books {
            filter.include_many(list.into_iter().map(BookFilter::new).collect());
        }

        if let Some(list) = args.exclude_testaments {
            filter.exclude_many(list);
        }

        if let Some(list) = args.exclude_genres {
            filter.exclude_many(list.into_iter().map(GenreFilter::new).collect());
        }

        if let Some(list) = args.exclude_books {
            filter.exclude_many(list.into_iter().map(BookFilter::new).collect());
        }

        if let Some(list) = args.inside {
            for value in list {
                filter.filter_inside(&value)?;
            }
        }

        if let Some(list) = args.outside {
            for value in list {
                filter.filter_outside(&value)?;
            }
        }

        let matcher = filter.create_matcher();
        let books = matcher.data().books();
        let context = if let Some(book) = &args.context_book {
            let book = books
                .search(book)
                .ok_or_else(|| format!("Unknown book {book:?}"))?;
            Some(BookContext::Book(book))
        } else if let Some(pattern) = &args.context_heading {
            Some(BookContext::headings(books, pattern)?)
        } else {
            None
        };
        Ok(match context {
            Some(context) => matcher.with_context(context),
            None => matcher,
        })
    }
}
