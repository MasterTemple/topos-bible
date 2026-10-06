use crate::{
    data::{bible_data::BibleData, books::BookId},
    segments::{
        autocomplete::incomplete::IncompleteSegment, grammar::SegmentList, passage::Segments,
        segment::Segment,
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
    let segments = Segments::from_nodes(complete);
    let chapter_verses = data.chapter_verses().get_chapter_verses(&book)?;
    let suggestions =
        IncompleteSegment::from_node(incomplete)?.suggest(chapter_verses, segments.last())?;

    Some(SegmentSuggestions {
        start: book_match.start(),
        book,
        segments,
        suggestions,
    })
}
