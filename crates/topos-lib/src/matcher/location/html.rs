use htmloc::{FragmentEngine, GenerateOptions, Selection, TextFragment};
use itertools::Itertools;

use crate::matcher::{
    bible_matcher::{BibleMatcher, MatchResult, Matcher},
    instance::BibleMatch,
    location::line_col::{ByteIndex, LineColLocation, Position},
};

impl From<Selection> for LineColLocation {
    fn from(value: Selection) -> Self {
        LineColLocation {
            start: Position {
                line: value.start.line,
                column: value.start.column,
            },
            end: Position {
                line: value.end.line,
                column: value.end.column,
            },
            bytes: ByteIndex::new(value.bytes.start, value.bytes.end),
        }
    }
}

impl From<LineColLocation> for Selection {
    fn from(val: LineColLocation) -> Self {
        Selection {
            start: htmloc::Position {
                line: val.start.line,
                column: val.start.column,
            },
            end: htmloc::Position {
                line: val.end.line,
                column: val.end.column,
            },
            bytes: htmloc::ByteIndex {
                start: val.bytes.start,
                end: val.bytes.end,
            },
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum HTMLMatchError {
    #[error("Failed to generate Text Fragment")]
    FailedToGenerate,
}

#[derive(Clone, Debug)]
pub struct HTMLLocation {
    pub line_col: LineColLocation,
    pub text_fragment: TextFragment,
}

impl Matcher for HTMLLocation {
    type Input<'a> = &'a str;

    fn search<'a>(
        matcher: &BibleMatcher,
        input: Self::Input<'a>,
    ) -> MatchResult<Vec<BibleMatch<Self>>> {
        let doc = FragmentEngine::from_html(input);

        let results = matcher.search::<LineColLocation>(doc.plain_text())?;

        results
            .into_iter()
            .map(|m| {
                let selection: Selection = m.location.into();
                let text_fragment = doc
                    .generate(selection, Some(GenerateOptions::default()))
                    .ok_or(HTMLMatchError::FailedToGenerate)?;
                Ok(m.map_loc(|line_col| HTMLLocation {
                    text_fragment,
                    line_col,
                }))
            })
            .try_collect()
    }
}
