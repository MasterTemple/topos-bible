use line_col::LineColLookup;
use regex::Match;

use crate::{
    data::{bible_data::BibleData, books::BookId},
    matcher::location::line_col::LineColLocation,
    segments::{
        grammar::SegmentList,
        passage::{Passage, Segments},
    },
};

// #[derive(Copy, Clone, Debug)]
// pub struct Position {
//     pub line: usize,
//     pub column: usize,
// }
//
// impl Position {
//     pub fn new(line: usize, column: usize) -> Self {
//         Self { line, column }
//     }
//     pub fn new_pair((line, column): (usize, usize)) -> Self {
//         Self::new(line, column)
//     }
// }
//
// #[derive(Copy, Clone, Debug)]
// pub struct Location {
//     pub start: Position,
//     pub end: Position,
// }
//
// impl Location {
//     pub fn new(lookup: &LineColLookup, start: usize, end: usize) -> Self {
//         let start = Position::new_pair(lookup.get(start));
//         let end = Position::new_pair(lookup.get(end));
//         Self { start, end }
//     }
// }

/**
- This is the minimal amount of data needed for a match in order to do complex filtering
- There will be a separate struct that will include file name, book name, book abbreviation, and so on

I think this is the ideal representation, because a matcher could have the capacity to get different locations
```ignore
pub trait Matcher<Location> {
    fn search(&self, input: PathOrContent) -> Option<Vec<BibleMatch<Location>>>;
}

impl Matcher<LineCol> for PlaintextMatcher {}
impl Matcher<TextFragment> for PlaintextMatcher {}

impl Matcher<Timestamp> for MediaMatcher {}

impl Matcher<Page> for PDFMatcher {}
impl Matcher<TextFragment> for PDFMatcher {}
```

but how will I parse, for example, timestamps from both SRT files and Whisper JSON files?
perhaps have additional args/params to the search method?
**no, make them part of the matcher struct**
create child structs if I need them: `WhisperJSONMediaMatcher` and `SRTMediaMatcher`

ideally, Location will be a big enum, so I don't have to deal with generics at the top level


*/
#[derive(Clone, Debug)]
pub struct BibleMatch<L = LineColLocation> {
    // TODO: make this into context, where Minimal<Location> is just the location, but
    // Verbose<Location> has the location and other things like the line, surrounding context
    // NOTE: I should make location (and maybe context type too) into enums
    pub location: L,
    /// I want this to be of type [`Passage`] so that way I can use the
    /// [`Passage::overlaps_with`] function
    pub psg: Passage,
}

impl<L> BibleMatch<L> {
    pub fn new(location: L, book_id: BookId, segments: Segments) -> Self {
        Self {
            location,
            psg: segments.with_book(book_id),
        }
    }
    pub fn map_loc<N>(self, f: impl FnOnce(L) -> N) -> BibleMatch<N> {
        BibleMatch {
            location: f(self.location),
            psg: self.psg,
        }
    }
}

impl BibleMatch {
    pub fn try_match<'a>(
        lookup: &LineColLookup,
        data: &'a BibleData,
        input: &str,
        cur: Match<'a>,
        next_start: Option<usize>,
    ) -> Option<Self> {
        let book_id = data.books().search(cur.as_str())?;

        let segment_window = match next_start {
            Some(next_start) => &input[cur.end()..next_start],
            None => &input[cur.end()..],
        };

        let list = SegmentList::parse(segment_window);
        let end = cur.end() + list.complete_end()?;
        let location = LineColLocation::new(lookup, cur.start(), end);
        let segments = Segments::from(&list);

        Some(BibleMatch::new(location, book_id, segments))
    }
}
