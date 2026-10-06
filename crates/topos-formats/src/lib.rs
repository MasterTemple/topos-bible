#![doc = include_str!("../README.md")]

use topos_bible::matcher::{BibleMatch, BibleMatcher};

#[cfg(feature = "epub")]
pub mod epub;
#[cfg(feature = "html")]
pub mod html;
#[cfg(feature = "html")]
pub mod htmloc;
#[cfg(feature = "json")]
pub mod json;
#[cfg(feature = "pdf")]
pub mod pdf;
#[cfg(feature = "srt")]
pub mod srt;
#[cfg(feature = "xml")]
pub mod xml;

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
    #[cfg(feature = "json")]
    #[error("JSON: {0}")]
    Json(#[from] json::JSONMatchError),
    #[cfg(feature = "xml")]
    #[error("XML: {0}")]
    Xml(#[from] xml::XMLMatchError),
}
