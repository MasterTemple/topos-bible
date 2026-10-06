use std::collections::BTreeSet;

use crate::{
    data::{bible_data::BibleData, books::BookId},
    matcher::instance::BibleMatch,
    segments::passage::Passage,
};

/// Decides which matches to keep, after every book has been matched
/// Decides which matches to keep, after every book has been matched
#[derive(Clone, Debug, Default)]
pub struct ComplexFilter {
    /// [`None`] keeps every book
    books: Option<BTreeSet<BookId>>,
    /// Keep matches entirely inside one of these
    inside_of: Vec<Passage>,
    /// Keep matches that share a verse with one of these
    overlapping: Vec<Passage>,
    /// Drop matches that share a verse with any of these
    outside_of: Vec<Passage>,
    /// Matches overlap only through the verses they name, not whole chapters (`John 3` doesn't
    /// overlap `John 3:16`, but `John 3:14-18` does), for `overlapping` and `outside_of`
    explicit_overlap: bool,
}

impl ComplexFilter {
    /// Only keep matches in these books
    pub fn books(&mut self, books: BTreeSet<BookId>) {
        self.books = Some(books);
    }

    pub fn inside(&mut self, psg: Passage) {
        self.inside_of.push(psg);
    }

    pub fn overlaps(&mut self, psg: Passage) {
        self.overlapping.push(psg);
    }

    pub fn outside(&mut self, psg: Passage) {
        self.outside_of.push(psg);
    }

    /// Whole chapters in matches don't count as overlapping (see the field)
    pub fn explicit_overlap(&mut self, explicit: bool) {
        self.explicit_overlap = explicit;
    }

    /**
    - Inside and overlapping passages are inclusions, joined with a logical OR: a match is kept
      if it is inside any `inside` passage or overlaps any `overlaps` passage
    - Then a match that overlaps any `outside` passage is dropped
    */
    pub fn keep(&self, psg: &Passage, data: &BibleData) -> bool {
        if self.books.as_ref().is_some_and(|b| !b.contains(&psg.book)) {
            return false;
        }
        let versification = data.chapter_verses().get_chapter_verses(&psg.book);
        // With explicit overlap, only the verses the match names can overlap
        let explicit = self.explicit_overlap.then(|| psg.explicit_verses());
        let overlap_part = match &explicit {
            Some(part) => part.as_ref(),
            None => Some(psg),
        };
        let overlaps = |other: &Passage| {
            overlap_part.is_some_and(|part| other.overlaps_passage(part, versification))
        };
        let included = (self.inside_of.is_empty() && self.overlapping.is_empty())
            || self
                .inside_of
                .iter()
                .any(|outer| outer.contains_passage(psg, versification))
            || self.overlapping.iter().any(overlaps);
        included && !self.outside_of.iter().any(overlaps)
    }

    /// The only books a kept match can be in, or [`None`] for any book: the allowed books,
    /// narrowed to the books of the inside and overlapping passages when there are any
    pub fn possible_books(&self) -> Option<BTreeSet<BookId>> {
        let mut books = self.books.clone();
        if !self.inside_of.is_empty() || !self.overlapping.is_empty() {
            let passages: BTreeSet<BookId> = (self.inside_of.iter())
                .chain(&self.overlapping)
                .map(|psg| psg.book)
                .collect();
            books = Some(match books {
                Some(books) => books.intersection(&passages).copied().collect(),
                None => passages,
            });
        }
        books
    }

    pub fn as_filter<'a>(&'a self, data: &'a BibleData) -> FilteredBibleMatches<'a> {
        FilteredBibleMatches::new(self, data)
    }
}

pub struct FilteredBibleMatches<'a> {
    filter: &'a ComplexFilter,
    data: &'a BibleData,
    matches: Vec<BibleMatch>,
}

impl<'a> FilteredBibleMatches<'a> {
    pub fn new(filter: &'a ComplexFilter, data: &'a BibleData) -> Self {
        Self {
            filter,
            data,
            matches: vec![],
        }
    }

    pub fn try_add(&mut self, m: BibleMatch) {
        if self.filter.keep(&m.psg, self.data) {
            self.matches.push(m);
        }
    }

    pub fn matches(self) -> Vec<BibleMatch> {
        self.matches
    }
}
