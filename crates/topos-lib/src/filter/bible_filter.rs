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

    /// Whether the included testaments, genres, and books have no book in common, like `OT` and
    /// `Pauline Epistles` (exclusions emptying the search don't count; that is what they ask for)
    pub fn is_contradictory(&self) -> bool {
        match (&self.scope, &self.included) {
            (Some(scope), Some(included)) => scope.is_disjoint(included),
            _ => false,
        }
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
        self.complex_filter.inside(psg);
        Ok(())
    }

    /// Keep matches that share any verse with this passage (`Err` if it cannot be parsed)
    pub fn filter_overlaps(&mut self, passage: &str) -> ToposResult<()> {
        let psg = self.parse_passage(passage)?;
        self.complex_filter.overlaps(psg);
        Ok(())
    }

    /// Drop matches that overlap this passage (`Err` if it cannot be parsed)
    pub fn filter_outside(&mut self, passage: &str) -> ToposResult<()> {
        let psg = self.parse_passage(passage)?;
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
