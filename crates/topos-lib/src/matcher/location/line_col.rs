use line_col::LineColLookup;
use regex::Match;

use crate::matcher::{
    bible_matcher::{BibleMatcher, MatchResult, Matcher},
    instance::BibleMatch,
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

        let mut prev: Option<Match<'_>> = None;
        let lookup = LineColLookup::new(input);
        // basically execute behind by 1 iteration (so I can see the start of the next match)
        for cur in matcher
            .data()
            .books()
            .candidate_regex()
            .captures_iter(input)
        {
            // this is just the book name
            let cur = cur.get(1).unwrap();
            if let Some(prev) = prev
                && let Some(m) = BibleMatch::try_match(
                    &lookup,
                    // self.data.as_ref(),
                    matcher.data(),
                    input,
                    prev,
                    Some(cur.start()),
                )
            {
                filtered.try_add(m);
            }
            prev = Some(cur);
        }

        // handle last one
        if let Some(prev) = prev
            && let Some(m) = BibleMatch::try_match(&lookup, matcher.data(), input, prev, None)
        {
            filtered.try_add(m);
        }

        let matches = filtered.matches();
        Ok(matches)
    }

    fn find<'a>(matcher: &BibleMatcher, input: Self::Input<'a>) -> Option<BibleMatch<Self>> {
        let mut filtered = matcher.filter();
        let lookup = LineColLookup::new(input);

        let first = matcher
            .data()
            .books()
            .candidate_regex()
            .captures_iter(input)
            .next()?
            .get(1)?;

        filtered.try_add(BibleMatch::try_match(
            &lookup,
            matcher.data(),
            input,
            first,
            None,
        )?);

        filtered.matches().into_iter().next()
    }
}
