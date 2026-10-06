use itertools::Itertools;
use std::cmp::Ordering;

use chumsky::{
    prelude::*,
    text::{Char, digits, inline_whitespace, newline},
};

use crate::matcher::{bible_matcher::Matcher, location::line_col::LineColLocation};

// TODO: Make this more general
#[derive(Clone, Copy, Debug)]
pub struct SRTLocation {
    pub id: u32,
    // TODO: Make this f32
    pub start: SRTTimeStamp,
    pub end: SRTTimeStamp,
}

#[derive(Clone, Debug)]
pub struct SRTDocument<'a> {
    segments: Vec<Spanned<SRTSegment<'a>>>,
}

#[derive(Clone, Debug)]
pub struct SRTSegment<'a> {
    pub id: u32,
    pub start: SRTTimeStamp,
    pub end: SRTTimeStamp,
    pub text: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct SRTTimeStamp {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub millis: u32,
}

fn num<'a>() -> impl Parser<'a, &'a str, u32> {
    digits(10)
        .to_slice()
        .from_str()
        .try_map(|v, _span| v.map_err(|_| EmptyErr::default()))
}

fn colon<'a>() -> impl Parser<'a, &'a str, char> {
    just(':')
}
fn comma<'a>() -> impl Parser<'a, &'a str, char> {
    just(',')
}

fn arrow<'a>() -> impl Parser<'a, &'a str, &'a str> {
    just("-->").padded_by(inline_whitespace())
}

impl SRTTimeStamp {
    pub fn parser<'a>() -> impl Parser<'a, &'a str, Self> {
        num()
            .then_ignore(colon())
            .then(num())
            .then_ignore(colon())
            .then(num())
            .then_ignore(comma())
            .then(num())
            .map(|(((hours, minutes), seconds), millis)| Self {
                hours,
                minutes,
                seconds,
                millis,
            })
    }
}

impl<'a> SRTSegment<'a> {
    pub fn parser() -> impl Parser<'a, &'a str, Self> {
        num()
            .then_ignore(newline())
            .then(SRTTimeStamp::parser())
            .then_ignore(arrow())
            .then(SRTTimeStamp::parser())
            .then_ignore(newline())
            .then(
                any()
                    .filter(|c: &char| !c.is_newline())
                    .repeated()
                    .at_least(1)
                    .to_slice(),
            )
            .then_ignore(newline())
            .map(|(((id, start), end), text)| Self {
                id,
                start,
                end,
                text,
            })
    }
}

// TODO: Test this and make more fault tolerant
impl<'a> SRTDocument<'a> {
    pub fn parser() -> impl Parser<'a, &'a str, Self> {
        SRTSegment::parser()
            .padded()
            .spanned()
            .repeated()
            .collect()
            .map(|segments| Self { segments })
    }

    pub fn find_containing_segment(&self, byte: usize) -> Option<&Spanned<SRTSegment<'_>>> {
        let res = self.segments.binary_search_by(|seg| {
            match (seg.span.start <= byte, byte <= seg.span.end) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                (false, false) => unreachable!(),
            }
        });
        match res {
            Ok(idx) => self.segments.get(idx),
            Err(_) => None,
        }
    }

    pub fn find_location(&self, byte: usize) -> Option<SRTLocation> {
        let seg = self.find_containing_segment(byte)?;
        Some(SRTLocation {
            id: seg.id,
            start: seg.start,
            end: seg.end,
        })
    }
}

impl Matcher for SRTLocation {
    type Input<'a> = &'a str;

    fn search<'a>(
        matcher: &crate::matcher::bible_matcher::BibleMatcher,
        input: Self::Input<'a>,
    ) -> crate::matcher::bible_matcher::MatchResult<Vec<crate::matcher::instance::BibleMatch<Self>>>
    {
        let results = matcher.search::<LineColLocation>(input)?;

        let doc = SRTDocument::parser()
            .parse(input)
            .into_result()
            .map_err(|_| SRTMatchError::Parse)?;

        results
            .into_iter()
            .map(|m| {
                let segment = doc
                    .find_location(m.location.bytes.start)
                    .ok_or(SRTMatchError::FindSegment)?;
                Ok(m.map_loc(|_line_col| segment))
            })
            .try_collect()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SRTMatchError {
    #[error("Failed to parse")]
    Parse,
    #[error("Failed to find segment")]
    FindSegment,
}
