use crate::{matcher::instance::BibleMatch, segments::passage::Passage};

#[derive(Clone, Debug, Default)]
pub struct ComplexFilter {
    inside_of: Vec<Passage>,
    outside_of: Vec<Passage>,
}

impl ComplexFilter {
    pub fn new(inside_of: Vec<Passage>, outside_of: Vec<Passage>) -> Self {
        Self {
            inside_of,
            outside_of,
        }
    }

    pub fn inside(&mut self, psg: Passage) {
        self.inside_of.push(psg);
    }

    pub fn outside(&mut self, psg: Passage) {
        self.outside_of.push(psg);
    }

    pub fn keep(&self, psg: &Passage) -> bool {
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
