use line_col::LineColLookup;

use crate::matcher::{
    bible_matcher::{BibleMatcher, MatchResult, Matcher},
    instance::{BibleMatch, FoundPassage},
    text::SearchText,
};

#[derive(Copy, Clone, Debug)]
pub struct ByteIndex {
    pub start: usize,
    pub end: usize,
}

impl ByteIndex {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
    pub fn new_pair((line, column): (usize, usize)) -> Self {
        Self::new(line, column)
    }
}

// TODO: I need start byte
#[derive(Copy, Clone, Debug)]
pub struct LineColLocation {
    pub start: Position,
    pub end: Position,
    pub bytes: ByteIndex,
}

impl LineColLocation {
    pub fn new(lookup: &LineColLookup, start: usize, end: usize) -> Self {
        let bytes = ByteIndex::new(start, end);
        let start = Position::new_pair(lookup.get(start));
        let end = Position::new_pair(lookup.get(end));
        Self { start, end, bytes }
    }
}

impl Matcher for LineColLocation {
    type Input<'a> = &'a str;

    /// - This always returns the [`Ok`] variant
    /// - Using the [`Result::unwrap_or_default()`] method results in an empty [`Vec`], so just do that
    fn search<'a>(
        matcher: &BibleMatcher,
        input: Self::Input<'a>,
    ) -> MatchResult<Vec<BibleMatch<Self>>> {
        let mut filtered = matcher.filter();
        let text = SearchText::new(input);
        let lookup = LineColLookup::new(input);

        let mut candidates = matcher.data().books().candidates(text.as_str()).peekable();
        while let Some(cur) = candidates.next() {
            let next_start = candidates.peek().map(|next| next.start());
            let Some(found) = FoundPassage::find(matcher.data(), text.as_str(), cur, next_start)
            else {
                continue;
            };
            let bytes = text.original_range(found.bytes);
            let location = LineColLocation::new(&lookup, bytes.start, bytes.end);
            filtered.try_add(BibleMatch {
                location,
                psg: found.psg,
            });
        }

        let matches = filtered.matches();
        Ok(matches)
    }
}
