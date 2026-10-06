use std::collections::BTreeSet;

use crate::{data::books::BookId, matcher::instance::BibleMatch, segments::passage::Passage};

/// Decides which matches to keep, after every book has been matched
#[derive(Clone, Debug, Default)]
pub struct ComplexFilter {
    /// [`None`] keeps every book
    books: Option<BTreeSet<BookId>>,
    inside_of: Vec<Passage>,
    outside_of: Vec<Passage>,
}

impl ComplexFilter {
    pub fn new(inside_of: Vec<Passage>, outside_of: Vec<Passage>) -> Self {
        Self {
            books: None,
            inside_of,
            outside_of,
        }
    }

    /// Only keep matches in these books
    pub fn books(&mut self, books: BTreeSet<BookId>) {
        self.books = Some(books);
    }

    pub fn inside(&mut self, psg: Passage) {
        self.inside_of.push(psg);
    }

    pub fn outside(&mut self, psg: Passage) {
        self.outside_of.push(psg);
    }

    pub fn keep(&self, psg: &Passage) -> bool {
        if self.books.as_ref().is_some_and(|b| !b.contains(&psg.book)) {
            return false;
        }

        let is_inside = self.inside_of.is_empty()
            || self
                .inside_of
                .iter()
                .any(|inside| inside.overlaps_with(psg));

        if !is_inside {
            return false;
        }

        self.outside_of
            .iter()
            .all(|outside| !outside.overlaps_with(psg))
    }

    pub fn as_filter<'a>(&'a self) -> FilteredBibleMatches<'a> {
        FilteredBibleMatches::new(self)
    }
}

pub struct FilteredBibleMatches<'a> {
    filter: &'a ComplexFilter,
    matches: Vec<BibleMatch>,
}

impl<'a> FilteredBibleMatches<'a> {
    pub fn new(filter: &'a ComplexFilter) -> Self {
        Self {
            filter,
            matches: vec![],
        }
    }

    pub fn try_add(&mut self, m: BibleMatch) {
        if self.filter.keep(&m.psg) {
            self.matches.push(m);
        }
    }

    pub fn matches(self) -> Vec<BibleMatch> {
        self.matches
    }
}
