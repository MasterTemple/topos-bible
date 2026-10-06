use crate::{
    data::books::BookId,
    segments::{passage::Segments, segment::Segment},
};

/// TODO: I want to be able to suggest incomplete segments, for example `1:1-` and then suggest
/// `1:1-2:`
///
/// TODO: For LSP purposes, I should include start location from input
pub struct CompletionOutput {
    pub start: usize,
    pub book: BookId,
    pub segments: Segments,
    pub suggestions: Vec<Segment>,
}

impl CompletionOutput {
    pub fn new(start: usize, book: BookId, segments: Segments, suggestions: Vec<Segment>) -> Self {
        Self {
            start,
            book,
            segments,
            suggestions,
        }
    }
}
