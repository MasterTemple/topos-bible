use crate::{
    data::bible_data::BibleData,
    filter::bible_filter::BibleFilter,
    matcher::{
        context::BookContext,
        instance::{BibleMatch, FoundPassage},
        line_col::{LineColLocation, LineIndex},
        matches::{ComplexFilter, FilteredBibleMatches},
        text::SearchText,
    },
};

#[derive(Clone, Debug)]
pub struct BibleMatcher {
    data: BibleData,
    /// Every book is always matched (so `1 John` is never mistaken for `John`), then filtered here
    complex_filter: ComplexFilter,
    /// When set, references without a book name (`1:1-5`) are matched too
    context: Option<BookContext>,
}

impl BibleMatcher {
    pub fn new(data: BibleData, complex_filter: ComplexFilter) -> Self {
        Self {
            data,
            complex_filter,
            context: None,
        }
    }

    /// Also match references without a book name, using this context for their book
    pub fn with_context(mut self, context: BookContext) -> Self {
        self.context = Some(context);
        self
    }

    pub fn context(&self) -> Option<&BookContext> {
        self.context.as_ref()
    }

    pub fn data(&self) -> &BibleData {
        &self.data
    }

    pub fn filter(&self) -> FilteredBibleMatches<'_> {
        self.complex_filter.as_filter(&self.data)
    }
}

impl BibleMatcher {
    /**
    Finds every reference in `text`
    - Every book name followed by a chapter is a candidate, and its segments end where the next
      candidate starts (so `John 1:1, 3 John 5` is `John 1:1` and `3 John 5`)
    - With a [`BookContext`], references without a book name are found too
    - Matches are then filtered by book and passage
    */
    pub fn search(&self, text: &str) -> Vec<BibleMatch> {
        let mut filtered = self.filter();
        let original = text;
        let text = SearchText::new(original);
        let index = LineIndex::new(original);
        let data = self.data();

        let starts: Vec<_> = data.books().candidates(text.as_str()).collect();
        let mut found: Vec<FoundPassage> = starts
            .iter()
            .enumerate()
            .filter_map(|(idx, cur)| {
                let next_start = starts.get(idx + 1).map(|next| next.start());
                FoundPassage::find(data, text.as_str(), *cur, next_start)
            })
            .collect();

        if let Some(context) = self.context() {
            let taken: Vec<_> = found.iter().map(|f| f.bytes.clone()).collect();
            let book_starts: Vec<_> = starts.iter().map(|s| s.start()).collect();
            found.extend(context.find_bare(data, text.as_str(), &taken, &book_starts));
            found.sort_by_key(|f| f.bytes.start);
        }

        for found in found {
            let bytes = text.original_range(found.bytes);
            let location = LineColLocation::new(&index, bytes.start, bytes.end);
            filtered.try_add(BibleMatch {
                location,
                psg: found.psg,
            });
        }
        filtered.matches()
    }

    /// The first reference in `text`
    pub fn find(&self, text: &str) -> Option<BibleMatch> {
        self.search(text).into_iter().next()
    }
}

impl Default for BibleMatcher {
    fn default() -> Self {
        BibleFilter::default().create_matcher()
    }
}
