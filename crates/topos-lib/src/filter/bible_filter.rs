use std::collections::BTreeSet;

use crate::{
    data::{bible_data::BibleData, books::BookId},
    error::{ToposError, ToposResult},
    matcher::{bible_matcher::BibleMatcher, matches::ComplexFilter},
    segments::Passage,
};

pub trait IsFilter {
    /// These are the ids that correspond to the argument, excluded or included
    fn get_ids(&self, data: &BibleData) -> BTreeSet<BookId>;
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
    fn get_ids(&self, data: &BibleData) -> BTreeSet<BookId> {
        self.inner().get_ids(data)
    }
}

#[derive(Clone)]
pub struct BibleFilter {
    data: BibleData,
    /// indicates whether or not there has been an inclusion, which implicitly calls an exclusion
    /// on all the original data
    /// i dont need to use this if an exclusion is called at the beginning, but then again, there
    /// is no point in doing that, unless i am only doing an exclusion
    has_done_an_inclusion: bool,
    ids: BTreeSet<BookId>,
    complex_filter: ComplexFilter,
}

impl BibleFilter {
    pub fn new(data: BibleData) -> Self {
        // this should start full
        let ids = data.books().ids().collect();
        let has_done_an_inclusion = false;
        let complex_filter = ComplexFilter::default();
        Self {
            data,
            has_done_an_inclusion,
            ids,
            complex_filter,
        }
    }

    pub fn push<T: IsFilter>(&mut self, op: Operation<T>) {
        let ids = op.get_ids(&self.data);

        match op {
            Operation::Include(_) => {
                if self.has_done_an_inclusion {
                    self.ids.extend(ids);
                } else {
                    self.ids = ids;
                    self.has_done_an_inclusion = true;
                }
            }
            Operation::Exclude(_) => {
                self.ids.retain(|id| !ids.contains(id));
            }
        };
    }

    pub fn with<T: IsFilter>(mut self, op: Operation<T>) -> BibleFilter {
        self.push(op);
        self
    }

    pub fn include<T: IsFilter>(&mut self, value: T) {
        self.push(Operation::Include(value));
    }

    pub fn include_many<T: IsFilter>(&mut self, list: Vec<T>) {
        for value in list {
            self.include(value);
        }
    }

    pub fn exclude<T: IsFilter>(&mut self, value: T) {
        self.push(Operation::Exclude(value));
    }

    pub fn exclude_many<T: IsFilter>(&mut self, list: Vec<T>) {
        for value in list {
            self.exclude(value);
        }
    }

    pub fn ids(&self) -> &BTreeSet<BookId> {
        &self.ids
    }

    /// Only keep matches that overlap this passage (`Err` if it cannot be parsed)
    pub fn filter_inside(&mut self, passage: &str) -> ToposResult<()> {
        let psg = self.parse_passage(passage)?;
        self.complex_filter.inside(psg);
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
    use crate::{
        filter::{bible_filter::BibleFilter, filters::book::BookFilter},
        matcher::location::line_col::LineColLocation,
    };

    fn search(filter: BibleFilter, input: &str) -> Vec<String> {
        let matcher = filter.create_matcher();
        matcher
            .search::<LineColLocation>(input)
            .unwrap()
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
        filter.include(BookFilter::new("John"));
        assert_eq!(
            search(filter, "1 John 2:1, John 3:16, 3 John 4"),
            ["John 3:16"]
        );
    }

    #[test]
    fn inside_filter() {
        let mut filter = BibleFilter::default();
        filter.filter_inside("Romans 8").unwrap();
        assert!(filter.filter_inside("not a passage").is_err());
        assert_eq!(
            search(filter, "Romans 8:28, Romans 9:1, John 8:1"),
            ["Romans 8:28"]
        );
    }
}
