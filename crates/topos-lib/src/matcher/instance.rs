use std::ops::Range;

use regex::Match;

use crate::{
    data::{bible_data::BibleData, books::BookId},
    matcher::line_col::LineColLocation,
    segments::{
        grammar::{NumberKind, SegmentList},
        passage::{Passage, Segments},
        resolve::Resolver,
    },
};

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

/// A passage found in text, before its location is computed
#[derive(Clone, Debug)]
pub struct FoundPassage {
    /// Byte range of the book name and segments in the searched text
    pub bytes: Range<usize>,
    pub psg: Passage,
}

/// Text without whitespace this long is encoded data or code (like base64 in an SVG), not prose
const LONG_TOKEN: usize = 64;

/// Whether the word around `at` is at least [`LONG_TOKEN`] bytes without whitespace
fn in_long_token(text: &str, at: usize) -> bool {
    let is_space = |b: &u8| b.is_ascii_whitespace();
    let before = text.as_bytes()[..at]
        .iter()
        .rev()
        .take(LONG_TOKEN)
        .take_while(|b| !is_space(b))
        .count();
    let after = text.as_bytes()[at..]
        .iter()
        .take(LONG_TOKEN)
        .take_while(|b| !is_space(b))
        .count();
    before + after >= LONG_TOKEN
}

/// Book names are written in lowercase, Title Case, or CAPS, never like `mK` or `jE`
fn book_case_is_plausible(name: &str) -> bool {
    name.split_whitespace().all(|word| {
        let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
        !letters
            .windows(2)
            .any(|pair| pair[0].is_lowercase() && pair[1].is_uppercase())
    })
}

impl FoundPassage {
    /**
    Parses the passage that starts with the book name `cur`
    - `next_start` is where the next book name starts, which ends this passage's segments
      (that is how `John 1:1, 3 John 5` becomes `John 1:1` and `3 John 5`)
    */
    pub fn find(
        data: &BibleData,
        text: &str,
        cur: Match<'_>,
        next_start: Option<usize>,
    ) -> Option<Self> {
        let book = data.books().search(cur.as_str())?;
        if in_long_token(text, cur.start()) || !book_case_is_plausible(cur.as_str()) {
            return None;
        }
        let window = &text[cur.end()..next_start.unwrap_or(text.len())];
        let list = SegmentList::parse(window);
        let (found, has_verse) = Self::resolve(data, book, cur.end(), &list)?;
        // Abbreviations that are also words (`is`) need an explicit verse (`Is 1:1`)
        if !has_verse && data.books().is_ambiguous(cur.as_str()) {
            return None;
        }
        // A book glued to its chapter (`Jn3:16`) needs a verse, and never a Roman numeral, since
        // codes and encoded data are full of things like `Gn5` and `GNi`
        let first = list.nodes.first()?;
        let glued = first.start.span.start == 0 && !cur.as_str().ends_with('.');
        if glued && (!has_verse || first.start.kind == NumberKind::Roman) {
            return None;
        }
        Some(Self {
            bytes: cur.start()..found.bytes.end,
            ..found
        })
    }

    /// A reference without a book name (`1:1-5`), which must start with `chapter:verse`
    pub fn find_bare(
        data: &BibleData,
        book: BookId,
        text: &str,
        window: Range<usize>,
    ) -> Option<Self> {
        let list = SegmentList::parse(&text[window.clone()]);
        let first = list.nodes.first()?;
        if first.start_verse.is_none_or(|p| p.delimiter.actual != ':') {
            return None;
        }
        Self::resolve(data, book, window.start, &list).map(|(found, _)| found)
    }

    /// Resolves segments parsed at byte `start`, and whether they have an explicit verse
    fn resolve(
        data: &BibleData,
        book: BookId,
        start: usize,
        list: &SegmentList,
    ) -> Option<(Self, bool)> {
        let versification = data.chapter_verses().get_chapter_verses(&book);
        let resolved = Resolver::for_book(versification).resolve(&list.nodes);
        if resolved.used == 0 {
            return None;
        }
        let has_verse = list.nodes[..resolved.used]
            .iter()
            .any(|node| node.start_verse.is_some());
        let found = Self {
            // Only the text that resolved is part of the match
            bytes: start..start + resolved.end,
            psg: resolved.segments.with_book(book),
        };
        Some((found, has_verse))
    }
}
