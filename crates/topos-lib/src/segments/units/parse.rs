use std::{iter::Peekable, str::Chars};

use itertools::Itertools;

use crate::{
    error::ToposError,
    segments::{passage::Segments, segment::Segment},
};

pub(crate) trait SegmentParseMethods: ParsableSegment {
    fn expect_done(chars: &mut Peekable<Chars<'_>>) -> Result<(), ToposError> {
        if chars.next().is_none() {
            Ok(())
        } else {
            Err(ToposError::ExpectedFormat(Self::EXPECTED_FORMAT))
        }
    }

    fn expect_char(chars: &mut Peekable<Chars<'_>>, char: char) -> Result<(), ToposError> {
        if chars.next().is_some_and(|c| c == char) {
            Ok(())
        } else {
            Err(ToposError::ExpectedFormat(Self::EXPECTED_FORMAT))
        }
    }

    /// It must be peekable to not consume the following element
    fn take_number(chars: &mut Peekable<Chars<'_>>) -> Result<u8, ToposError> {
        chars
            .peeking_take_while(char::is_ascii_digit)
            .join("")
            .parse::<u8>()
            .map_err(|_| ToposError::ExpectedFormat(Self::EXPECTED_FORMAT))
    }
}
impl<T: ParsableSegment> SegmentParseMethods for T {}

pub trait ParsableSegment: Sized + TryFrom<Segment, Error = ToposError> {
    const EXPECTED_FORMAT: &'static str;

    /// - This is meant to be a strict match because this is to be highly performant method (since
    ///   this will be used for serialization)
    /// - If you would like a 'forgiving' parse method, use [`ParsableSegment::parse`]
    ///   which will call this method, but if it fails, then try to parse all segments,
    ///   take the first one, and coerce it when able
    fn parse_strict(input: &str) -> Result<Self, ToposError>;

    /// - This first calls [`ParsableSegment::parse_strict`] and if it fails, tries parsing
    ///   entire set of passage segments of all kinds (with all the character replacements)
    ///   and then match on the first segment or try and coerce it into the desired type
    /// - There must only be **exactly 1** segment matched
    fn parse(input: &str) -> Result<Self, ToposError> {
        Self::parse_strict(input).or_else(|_| {
            let segments =
                Segments::parse(input).ok_or(ToposError::ExpectedFormat(Self::EXPECTED_FORMAT))?;
            if segments.len() > 1 {
                return Err(ToposError::MultipleSegments(segments.len()));
            }
            Self::try_from(segments[0])
        })
    }
}
