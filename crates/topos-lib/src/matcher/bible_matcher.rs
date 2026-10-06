use regex::Regex;

use crate::{
    data::bible_data::BibleData,
    filter::bible_filter::BibleFilter,
    matcher::{
        instance::BibleMatch,
        location::{epub::EPUBMatchError, html::HTMLMatchError, srt::SRTMatchError},
        matches::{ComplexFilter, FilteredBibleMatches},
    },
    segments::autocomplete::input::InputAutoCompleter,
};

#[cfg(feature = "pdf")]
use crate::matcher::location::pdf::PDFMatchError;

#[derive(Clone, Debug)]
pub struct BibleMatcher {
    data: BibleData,
    /// The books to **not** match on **aren't** in this RegEx, so I won't process unnecessary books
    pub filtered_books: Regex,
    /// These are so I can check if the matches overlap with these
    complex_filter: ComplexFilter,
}

// TODO: I should have a search method for each type of Location
impl BibleMatcher {
    pub fn new(data: BibleData, filtered_books: Regex, complex_filter: ComplexFilter) -> Self {
        Self {
            data,
            filtered_books,
            complex_filter,
        }
    }

    pub fn data(&self) -> &BibleData {
        &self.data
    }

    pub fn filter(&self) -> FilteredBibleMatches<'_> {
        self.complex_filter.as_filter()
    }

    pub fn completer(&self) -> InputAutoCompleter<'_> {
        InputAutoCompleter::new(self)
    }
}

pub type MatchResult<T> = core::result::Result<T, MatchError>;

#[derive(thiserror::Error, Debug)]
pub enum MatchError {
    #[error("EPUB: {0}")]
    EPUB(#[from] EPUBMatchError),
    #[error("SRT: {0}")]
    SRT(#[from] SRTMatchError),
    #[error("HTML: {0}")]
    HTML(#[from] HTMLMatchError),
    #[cfg(feature = "pdf")]
    #[error("PDF: {0}")]
    PDF(#[from] PDFMatchError),
    #[error("{0}")]
    Unknown(Box<dyn std::error::Error>),
}

/**
- This is a trait that allows for generic location matching
- The [`find`](Matcher::find) method will by default use [`search`](Matcher::search) method and take the first result
*/
/*
TODO: I should return a result
- But line-column searches do not return a result -> `.ok().unwrap_or_default()`
*/
/*
TODO: I should allow parameters?
- Let user specify text fragment options
- Let user specify certain page of PDF to read
*/
pub trait Matcher: Sized {
    type Input<'a>;
    fn search<'a>(
        matcher: &BibleMatcher,
        input: Self::Input<'a>,
    ) -> MatchResult<Vec<BibleMatch<Self>>>;
    fn find<'a>(matcher: &BibleMatcher, input: Self::Input<'a>) -> Option<BibleMatch<Self>> {
        Self::search(matcher, input).ok()?.into_iter().next()
    }
}

impl BibleMatcher {
    pub fn search<'a, L: Matcher>(&self, input: L::Input<'a>) -> MatchResult<Vec<BibleMatch<L>>> {
        L::search(self, input)
    }

    pub fn find<'a, L: Matcher>(&self, input: L::Input<'a>) -> Option<BibleMatch<L>> {
        L::find(self, input)
    }
}

impl Default for BibleMatcher {
    fn default() -> Self {
        BibleFilter::default()
            .create_matcher()
            .expect("The default provided matcher should always compile")
    }
}
