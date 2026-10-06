//! Search documents for Bible references and report where each one is in that document's terms:
//! a text fragment for HTML, a cue and timestamps for subtitles, a CFI for EPUB, a page and
//! rectangles for PDF.
//!
//! Each format is behind a feature of the same name (`html`, `srt`, and `epub` by default; `pdf`
//! needs MuPDF).

use topos_lib::matcher::{BibleMatch, BibleMatcher};

#[cfg(feature = "epub")]
pub mod epub;
#[cfg(feature = "html")]
pub mod html;
#[cfg(feature = "pdf")]
pub mod pdf;
#[cfg(feature = "srt")]
pub mod srt;

/// A location type that a document format can report matches with
pub trait Format: Sized {
    /// What is searched (document text, or a path for container formats like EPUB)
    type Input<'a>;

    fn search(
        matcher: &BibleMatcher,
        input: Self::Input<'_>,
    ) -> Result<Vec<BibleMatch<Self>>, FormatError>;
}

/// Adds format-aware searching to [`BibleMatcher`]
pub trait SearchFormat {
    /// `matcher.search_format::<SRTLocation>(srt)`
    fn search_format<F: Format>(
        &self,
        input: F::Input<'_>,
    ) -> Result<Vec<BibleMatch<F>>, FormatError>;
}

impl SearchFormat for BibleMatcher {
    fn search_format<F: Format>(
        &self,
        input: F::Input<'_>,
    ) -> Result<Vec<BibleMatch<F>>, FormatError> {
        F::search(self, input)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum FormatError {
    #[cfg(feature = "epub")]
    #[error("EPUB: {0}")]
    Epub(#[from] epub::EPUBMatchError),
    #[cfg(feature = "srt")]
    #[error("SRT: {0}")]
    Srt(#[from] srt::SRTMatchError),
    #[cfg(feature = "html")]
    #[error("HTML: {0}")]
    Html(#[from] html::HTMLMatchError),
    #[cfg(feature = "pdf")]
    #[error("PDF: {0}")]
    Pdf(#[from] pdf::PDFMatchError),
}
