//! The one grammar for segment text (everything after a book name), shared by search,
//! autocomplete, and (later) formatting.
//!
//! - [`lexer`] turns text into numbers and delimiters with byte spans
//! - [`tree`] parses those into a lossless [`SegmentList`], keeping partial input like `1:1-`
//! - [`Segments`](crate::segments::passage::Segments) resolves a list into chapters and verses

pub mod lexer;
pub mod roman;
pub mod tree;

pub use tree::{
    Delimiter, DelimiterKind, Following, Number, NumberKind, Part, SegmentList, SegmentNode, Span,
};
