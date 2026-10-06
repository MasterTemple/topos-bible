use std::collections::BTreeSet;

use crate::{
    data::{bible_data::BibleData, books::BookId},
    error::{ToposError, ToposResult},
    matcher::{bible_matcher::BibleMatcher, matches::ComplexFilter},
    segments::Passage,
};

pub trait IsFilter {
    /// The books this filter names (an error if the name is unknown)
    fn get_ids(&self, data: &BibleData) -> ToposResult<BTreeSet<BookId>>;

    /// Whether including this narrows the other inclusions (testaments) instead of adding books
    /// to them (genres and books)
    fn narrows(&self) -> bool {
        false
    }
}

pub enum Operation<T> {
    Include(T),
    Exclude(T),
}

impl<T> Operation<T> {
    pub fn inner(&self) -> &T {
        match self {
            Operation::Include(t) => t,
            Operation::Exclude(t) => t,
        }
    }
}

impl<T: IsFilter> IsFilter for Operation<T> {
    fn get_ids(&self, data: &BibleData) -> ToposResult<BTreeSet<BookId>> {
        self.inner().get_ids(data)
    }

    fn narrows(&self) -> bool {
        self.inner().narrows()
    }
}

/// Which books to search:
///
/// - Included testaments limit the search to those testaments
/// - Included genres and books are added together, within those testaments
/// - Excluded testaments, genres, and books are always removed, whatever the order
///
/// So `NT` + `Pauline Epistles` is Paul's letters, `Pentateuch` + `Revelation` is six books, and
/// `OT` + `Pauline Epistles` is nothing (see [`BibleFilter::is_contradictory`]).

#[derive(Clone)]
pub struct BibleFilter {
    data: BibleData,
    /// Books in the included testaments, if any were included
    scope: Option<BTreeSet<BookId>>,
    /// Included genres and books, if any were included
    included: Option<BTreeSet<BookId>>,
    excluded: BTreeSet<BookId>,
    /// The books to search, kept up to date as filters are pushed
    ids: BTreeSet<BookId>,
    complex_filter: ComplexFilter,
    /// `--inside` and `--overlaps` passages as given, to explain contradictions
    included_passages: Vec<(String, Passage)>,
    /// `--outside` passages as given
    outside_passages: Vec<(String, Passage)>,
}

impl BibleFilter {
    pub fn new(data: BibleData) -> Self {
        let ids = data.books().ids().collect();
        Self {
            data,
            scope: None,
            included: None,
            excluded: BTreeSet::new(),
            ids,
            complex_filter: ComplexFilter::default(),
            included_passages: vec![],
            outside_passages: vec![],
        }
    }

    pub fn push<T: IsFilter>(&mut self, op: Operation<T>) -> ToposResult<()> {
        let ids = op.get_ids(&self.data)?;
        match op {
            Operation::Include(filter) if filter.narrows() => {
                self.scope.get_or_insert_default().extend(ids)
            }
            Operation::Include(_) => self.included.get_or_insert_default().extend(ids),
            Operation::Exclude(_) => self.excluded.extend(ids),
        };
        self.ids = self
            .data
            .books()
            .ids()
            .filter(|id| self.scope.as_ref().is_none_or(|scope| scope.contains(id)))
            .filter(|id| self.included.as_ref().is_none_or(|inc| inc.contains(id)))
            .filter(|id| !self.excluded.contains(id))
            .collect();
        Ok(())
    }

    /// Whether the filters contradict each other, so nothing can match (see [`Self::contradiction`])
    pub fn is_contradictory(&self) -> bool {
        self.contradiction().is_some()
    }

    /// Why nothing can match, when inclusions contradict each other or the exclusions (exclusions
    /// emptying the search don't count; that is what they ask for):
    ///
    /// - the included genres and books are outside the included testaments (`OT` and
    ///   `Pauline Epistles`)
    /// - every inside or overlapping passage is in a book that isn't searched (`-b Genesis -i
    ///   "Romans 8"`)
    /// - every inside or overlapping passage is within an outside passage (`-i "John 3:16"
    ///   --outside "John 3"`)
    pub fn contradiction(&self) -> Option<String> {
        if let (Some(scope), Some(included)) = (&self.scope, &self.included)
            && scope.is_disjoint(included)
        {
            return Some("no included genre or book is in the included testaments".into());
        }
        if self.included_passages.is_empty() {
            return None;
        }
        let names = |passages: &[&(String, Passage)]| {
            let names = passages
                .iter()
                .map(|(text, _)| text.as_str())
                .collect::<Vec<_>>();
            let verb = if names.len() == 1 { "is" } else { "are" };
            (names.join(", "), verb)
        };
        let searched: Vec<_> = self
            .included_passages
            .iter()
            .filter(|(_, psg)| self.ids.contains(&psg.book))
            .collect();
        if searched.is_empty() {
            let all: Vec<_> = self.included_passages.iter().collect();
            let (names, verb) = names(&all);
            return Some(format!("{names} {verb} in books that aren't searched"));
        }
        let covered = searched.iter().all(|(_, psg)| {
            let versification = self.data.chapter_verses().get_chapter_verses(&psg.book);
            self.outside_passages
                .iter()
                .any(|(_, outside)| outside.contains_passage(psg, versification))
        });
        if covered {
            let (names, verb) = names(&searched);
            let outside = self
                .outside_passages
                .iter()
                .map(|(text, _)| text.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Some(format!(
                "{names} {verb} within the outside passages ({outside})"
            ));
        }
        None
    }

    pub fn with<T: IsFilter>(mut self, op: Operation<T>) -> ToposResult<BibleFilter> {
        self.push(op)?;
        Ok(self)
    }

    pub fn include<T: IsFilter>(&mut self, value: T) -> ToposResult<()> {
        self.push(Operation::Include(value))
    }

    pub fn include_many<T: IsFilter>(
        &mut self,
        list: impl IntoIterator<Item = T>,
    ) -> ToposResult<()> {
        list.into_iter().try_for_each(|value| self.include(value))
    }

    pub fn exclude<T: IsFilter>(&mut self, value: T) -> ToposResult<()> {
        self.push(Operation::Exclude(value))
    }

    pub fn exclude_many<T: IsFilter>(
        &mut self,
        list: impl IntoIterator<Item = T>,
    ) -> ToposResult<()> {
        list.into_iter().try_for_each(|value| self.exclude(value))
    }

    pub fn ids(&self) -> &BTreeSet<BookId> {
        &self.ids
    }

    /// Keep matches entirely inside this passage (`Err` if it cannot be parsed)
    pub fn filter_inside(&mut self, passage: &str) -> ToposResult<()> {
        let psg = self.parse_passage(passage)?;
        self.included_passages
            .push((passage.to_string(), psg.clone()));
        self.complex_filter.inside(psg);
        Ok(())
    }

    /// Keep matches that share any verse with this passage (`Err` if it cannot be parsed)
    pub fn filter_overlaps(&mut self, passage: &str) -> ToposResult<()> {
        let psg = self.parse_passage(passage)?;
        self.included_passages
            .push((passage.to_string(), psg.clone()));
        self.complex_filter.overlaps(psg);
        Ok(())
    }

    /// Matches overlap (for `filter_overlaps` and `filter_outside`) only through the verses they
    /// name: a whole chapter like `John 3` doesn't overlap `John 3:16`, but `John 3:14-18` does
    pub fn explicit_overlap(&mut self, explicit: bool) {
        self.complex_filter.explicit_overlap(explicit);
    }

    /// Drop matches that overlap this passage (`Err` if it cannot be parsed)
    pub fn filter_outside(&mut self, passage: &str) -> ToposResult<()> {
        let psg = self.parse_passage(passage)?;
        self.outside_passages
            .push((passage.to_string(), psg.clone()));
        self.complex_filter.outside(psg);
        Ok(())
    }

    fn parse_passage(&self, passage: &str) -> ToposResult<Passage> {
        self.data
            .books()
            .parse(passage)
            .ok_or_else(|| ToposError::InvalidPassage(passage.to_string()))
    }

    pub fn create_matcher(mut self) -> BibleMatcher {
        self.complex_filter.books(self.ids);
        BibleMatcher::new(self.data, self.complex_filter)
    }
}

impl Default for BibleFilter {
    fn default() -> Self {
        Self::new(BibleData::default())
    }
}

#[cfg(test)]
mod tests {
    use crate::filter::{
        bible_filter::BibleFilter,
        filters::{book::BookFilter, genre::GenreFilter, testament::TestamentFilter},
    };

    fn search(filter: BibleFilter, input: &str) -> Vec<String> {
        let matcher = filter.create_matcher();
        matcher
            .search(input)
            .into_iter()
            .map(|m| {
                let book = matcher.data().books().get_name(m.psg.book).unwrap();
                format!("{book} {}", m.psg.segments)
            })
            .collect()
    }

    /// Issue #8: filtering for `John` used to match the `John` inside `1 John`
    #[test]
    fn book_filter_does_not_split_numbered_books() {
        let mut filter = BibleFilter::default();
        filter.include(BookFilter::new("John")).unwrap();
        assert_eq!(
            search(filter, "1 John 2:1, John 3:16, 3 John 4"),
            ["John 3:16"]
        );
    }

    #[test]
    fn inside_and_overlaps_filters() {
        let text = "Romans 8:28, Romans 8:38-9:1, Romans 9:2, John 8:1";
        let mut inside = BibleFilter::default();
        inside.filter_inside("Romans 8").unwrap();
        assert!(inside.filter_inside("not a passage").is_err());
        assert_eq!(search(inside, text), ["Romans 8:28"]);

        let mut overlaps = BibleFilter::default();
        overlaps.filter_overlaps("Romans 8").unwrap();
        assert_eq!(search(overlaps, text), ["Romans 8:28", "Romans 8:38-9:1"]);

        // Inclusions are joined with OR; exclusions apply after
        let mut both = BibleFilter::default();
        both.filter_inside("Romans 9").unwrap();
        both.filter_overlaps("John 8").unwrap();
        both.filter_outside("Romans 9:2").unwrap();
        assert_eq!(search(both, text), ["John 8:1"]);
    }

    fn books(filter: &BibleFilter) -> Vec<u8> {
        filter.ids().iter().map(|id| id.0).collect()
    }

    #[test]
    fn testaments_narrow_genres_and_books() {
        let mut filter = BibleFilter::default();
        filter.include(TestamentFilter::New).unwrap();
        filter
            .include(GenreFilter::new("Pauline Epistles"))
            .unwrap();
        assert_eq!(books(&filter), (45..=57).collect::<Vec<_>>());
        assert!(!filter.is_contradictory());

        // In either order
        let mut filter = BibleFilter::default();
        filter
            .include(GenreFilter::new("Pauline Epistles"))
            .unwrap();
        filter.include(TestamentFilter::New).unwrap();
        assert_eq!(books(&filter), (45..=57).collect::<Vec<_>>());
    }

    #[test]
    fn genres_and_books_add_up() {
        let mut filter = BibleFilter::default();
        filter.include(GenreFilter::new("Pentateuch")).unwrap();
        filter.include(BookFilter::new("Revelation")).unwrap();
        assert_eq!(books(&filter), [1, 2, 3, 4, 5, 66]);
        filter.include(TestamentFilter::Old).unwrap();
        assert_eq!(books(&filter), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn explicit_overlap_ignores_whole_chapters() {
        let text = "John 3, John 2-4, John 3:14-18, John 3:16-4:2, John 2; 3:16, John 3:1-5";
        let found = |explicit: bool, overlaps: &[&str], outside: &[&str]| {
            let mut filter = BibleFilter::default();
            filter.explicit_overlap(explicit);
            for passage in overlaps {
                filter.filter_overlaps(passage).unwrap();
            }
            for passage in outside {
                filter.filter_outside(passage).unwrap();
            }
            search(filter, text)
        };
        assert_eq!(
            found(false, &["John 3:16"], &[]),
            [
                "John 3",
                "John 2-4",
                "John 3:14-18",
                "John 3:16-4:2",
                "John 2; 3:16"
            ]
        );
        assert_eq!(
            found(true, &["John 3:16"], &[]),
            ["John 3:14-18", "John 3:16-4:2", "John 2; 3:16"]
        );
        // The passages given still cover whole chapters: `John 3` overlaps any verse in it
        assert_eq!(
            found(true, &["John 3"], &[]),
            [
                "John 3:14-18",
                "John 3:16-4:2",
                "John 2; 3:16",
                "John 3:1-5"
            ]
        );
        // --outside only drops matches that name one of its verses
        assert_eq!(
            found(true, &[], &["John 3:16"]),
            ["John 3", "John 2-4", "John 3:1-5"]
        );
    }

    #[test]
    fn contradictory_passages() {
        let mut filter = BibleFilter::default();
        filter.include(BookFilter::new("Genesis")).unwrap();
        filter.filter_inside("Romans 8").unwrap();
        assert_eq!(
            filter.contradiction().as_deref(),
            Some("Romans 8 is in books that aren't searched")
        );
        // One passage in a searched book is enough
        filter.filter_overlaps("Gen 1").unwrap();
        assert_eq!(filter.contradiction(), None);

        let mut filter = BibleFilter::default();
        filter.filter_inside("John 3:16").unwrap();
        filter.filter_overlaps("John 3:1-5").unwrap();
        filter.filter_outside("John 3").unwrap();
        assert_eq!(
            filter.contradiction().as_deref(),
            Some("John 3:16, John 3:1-5 are within the outside passages (John 3)")
        );
        // Partly outside still leaves something to find
        let mut filter = BibleFilter::default();
        filter.filter_overlaps("John 3-4").unwrap();
        filter.filter_outside("John 3").unwrap();
        assert_eq!(filter.contradiction(), None);
    }

    #[test]
    fn contradictory_inclusions() {
        let mut filter = BibleFilter::default();
        filter.include(TestamentFilter::Old).unwrap();
        filter
            .include(GenreFilter::new("Pauline Epistles"))
            .unwrap();
        assert!(books(&filter).is_empty());
        assert!(filter.is_contradictory());

        // Excluding everything is not a contradiction
        let mut filter = BibleFilter::default();
        filter.exclude(TestamentFilter::Old).unwrap();
        filter.exclude(TestamentFilter::New).unwrap();
        assert!(books(&filter).is_empty());
        assert!(!filter.is_contradictory());
    }
}
