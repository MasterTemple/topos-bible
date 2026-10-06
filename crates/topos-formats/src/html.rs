use crate::htmloc::{FragmentEngine, GenerateOptions, Selection, TextFragment};
use itertools::Itertools;

use topos_bible::matcher::{BibleMatch, BibleMatcher, LineColLocation};

use crate::{Format, FormatError};

fn selection(val: LineColLocation) -> Selection {
    {
        Selection {
            start: crate::htmloc::Position {
                line: val.start.line,
                column: val.start.column,
            },
            end: crate::htmloc::Position {
                line: val.end.line,
                column: val.end.column,
            },
            bytes: crate::htmloc::ByteIndex {
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

impl Format for HTMLLocation {
    type Input<'a> = &'a str;

    fn search(
        matcher: &BibleMatcher,
        input: Self::Input<'_>,
    ) -> Result<Vec<BibleMatch<Self>>, FormatError> {
        let doc = FragmentEngine::from_html(input);

        let results = matcher.search(doc.plain_text());

        results
            .into_iter()
            .map(|m| {
                let selection = selection(m.location);
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
