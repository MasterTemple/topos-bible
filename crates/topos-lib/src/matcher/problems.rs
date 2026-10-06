use std::ops::Range;

use crate::{
    matcher::{BibleMatcher, text::SearchText},
    segments::{grammar::SegmentList, resolve::Resolver},
};

/// A reference that is written like one but does not exist (`John 3:99`)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// Byte range of the segment that does not exist
    pub bytes: Range<usize>,
    pub message: String,
}

impl BibleMatcher {
    /**
    Segments that parse but do not exist in their book, like `John 3:99` or `John 3:16, 4:99`
    - Only segments with an explicit `chapter:verse` are reported, so prose like the date in
      `Mar 25, 2025` is left alone
    */
    pub fn problems(&self, text: &str) -> Vec<Problem> {
        let data = self.data();
        let text = SearchText::new(text);
        let starts: Vec<_> = data.books().candidates(text.as_str()).collect();
        let mut problems = vec![];
        for (idx, cur) in starts.iter().enumerate() {
            let Some(book) = data.books().search(cur.as_str()) else {
                continue;
            };
            let end = starts
                .get(idx + 1)
                .map_or(text.as_str().len(), |n| n.start());
            let list = SegmentList::parse(&text.as_str()[cur.end()..end]);
            let versification = data.chapter_verses().get_chapter_verses(&book);
            let valid = Resolver::for_book(versification).resolve(&list.nodes);
            let parsed = Resolver::default().resolve(&list.nodes);
            if parsed.used <= valid.used {
                continue;
            }
            let node = &list.nodes[valid.used];
            if node.start_verse.is_none() {
                continue;
            }
            let (Some(segment), Some(name)) =
                (parsed.segments.get(valid.used), data.books().get_name(book))
            else {
                continue;
            };
            let bytes = cur.end() + node.start.span.start..cur.end() + node.complete_end();
            problems.push(Problem {
                bytes: text.original_range(bytes),
                message: format!("{name} {segment} does not exist"),
            });
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problems(text: &str) -> Vec<(String, String)> {
        BibleMatcher::default()
            .problems(text)
            .into_iter()
            .map(|p| (text[p.bytes].to_string(), p.message))
            .collect()
    }

    #[test]
    fn reports_segments_that_do_not_exist() {
        assert_eq!(
            problems("See John 3:99."),
            [("3:99".into(), "John 3:99 does not exist".into())]
        );
        assert_eq!(
            problems("John 3:16, 4:99"),
            [("4:99".into(), "John 4:99 does not exist".into())]
        );
        assert_eq!(
            problems("Jude 2:1"),
            [("2:1".into(), "Jude 2:1 does not exist".into())]
        );
    }

    #[test]
    fn ignores_valid_references_and_prose() {
        assert!(problems("John 3:16, Jude 5, Mar 25, 2025, is 1 of").is_empty());
        assert!(problems("Mark 16, 25").is_empty());
    }
}
