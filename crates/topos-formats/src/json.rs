use std::ops::Range;

use serde_json::Value;
use topos_bible::matcher::{BibleMatch, BibleMatcher};

use crate::{Format, FormatError};

/// Where a match is in a JSON document: which string, and where in that string
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JSONLocation {
    /// A JSON Pointer (RFC 6901) to the string, like `/segments/3/text`
    pub pointer: String,
    /// Byte range of the match within the decoded string
    pub range: Range<usize>,
}

#[derive(thiserror::Error, Debug)]
pub enum JSONMatchError {
    #[error("invalid JSON: {0}")]
    Parse(#[from] serde_json::Error),
}

impl Format for JSONLocation {
    type Input<'a> = &'a str;

    /// Every string value is searched on its own (so a reference split across strings is missed)
    fn search(
        matcher: &BibleMatcher,
        input: Self::Input<'_>,
    ) -> Result<Vec<BibleMatch<Self>>, FormatError> {
        let value: Value = serde_json::from_str(input).map_err(JSONMatchError::from)?;
        let mut matches = vec![];
        search_value(matcher, &value, &mut String::new(), &mut matches);
        Ok(matches)
    }
}

fn search_value(
    matcher: &BibleMatcher,
    value: &Value,
    pointer: &mut String,
    matches: &mut Vec<BibleMatch<JSONLocation>>,
) {
    let mut descend = |key: &str, value: &Value, pointer: &mut String| {
        let len = pointer.len();
        pointer.push('/');
        pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
        search_value(matcher, value, pointer, matches);
        pointer.truncate(len);
    };
    match value {
        Value::String(text) => {
            matches.extend(matcher.search(text).into_iter().map(|m| {
                let bytes = m.location.bytes;
                m.map_loc(|_| JSONLocation {
                    pointer: pointer.clone(),
                    range: bytes.start..bytes.end,
                })
            }));
        }
        Value::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                descend(&idx.to_string(), item, pointer);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                descend(key, item, pointer);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_strings() {
        let json = r#"{"segments": [{"start": 1.5, "text": "intro"}, {"text": "See John 3:16"}],
                      "a/b": ["Rom 8:28"]}"#;
        let matches = JSONLocation::search(&BibleMatcher::default(), json).unwrap();
        let locations: Vec<_> = matches.iter().map(|m| m.location.clone()).collect();
        assert_eq!(
            locations,
            [
                JSONLocation {
                    pointer: "/a~1b/0".into(),
                    range: 0..8
                },
                JSONLocation {
                    pointer: "/segments/1/text".into(),
                    range: 4..13
                },
            ]
        );
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(JSONLocation::search(&BibleMatcher::default(), "{").is_err());
    }
}
