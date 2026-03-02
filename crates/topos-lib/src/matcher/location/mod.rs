pub mod line_col;

/// This is for text fragments
pub mod html;

pub mod srt;

/// https://github.com/Govcraft/vtt
pub mod vtt;

#[cfg(feature = "pdf")]
pub mod pdf;

pub mod epub;

// pub mod json;

// #[non_exhaustive]
pub enum AnyLocation {
    // LineCol(LineColLocation)
}
