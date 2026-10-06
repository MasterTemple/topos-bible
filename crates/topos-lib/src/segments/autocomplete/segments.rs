use crate::{
    data::{bible_data::BibleData, books::BookId},
    segments::{
        autocomplete::incomplete::IncompleteSegment, grammar::SegmentList, passage::Segments,
        resolve::Resolver, segment::Segment,
    },
};

/// What can complete the reference that ends the input
pub(crate) struct SegmentSuggestions {
    /// Where the book name starts
    pub start: usize,
    pub book: BookId,
    /// Segments that are already complete
    pub segments: Segments,
    /// Each one completes the segment being typed
    pub suggestions: Vec<Segment>,
}

/// Suggests segments for the reference at the end of `input` (the cursor), using the same
/// grammar as search
pub(crate) fn suggest_segments(data: &BibleData, input: &str) -> Option<SegmentSuggestions> {
    let cap = data.books().book_regex().captures_iter(input).last()?;
    let book_match = cap.get(1)?;
    let book = data.books().search(book_match.as_str())?;

    let segments_input = &input[book_match.end()..];
    let list = SegmentList::parse(segments_input);
    if !segments_input[list.end()..].trim().is_empty() {
        // The cursor is not inside a reference
        return None;
    }
    let (complete, incomplete) = list.split_incomplete();
    let chapter_verses = data.chapter_verses().get_chapter_verses(&book)?;
    // Resolved like search, so single-chapter books read numbers as verses
    let resolver = Resolver::for_book(Some(chapter_verses));
    let segments = resolver.resolve(complete).segments;
    // Whether the segment being typed starts with a verse: the separator before it (or after
    // the last segment, if nothing is typed yet) and what came before decide, as in search
    let separator = match incomplete {
        Some(node) => node.separator,
        None => list.trailing_separator,
    };
    let verse_chapter = resolver.next_verse_chapter(
        separator.map(|s| s.actual),
        complete.last(),
        segments.last(),
    );
    let suggestions = IncompleteSegment::from_node(incomplete)?.suggest(
        chapter_verses,
        segments.last(),
        verse_chapter,
    )?;

    Some(SegmentSuggestions {
        start: book_match.start(),
        book,
        segments,
        suggestions,
    })
}
