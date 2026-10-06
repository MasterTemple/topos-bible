use crate::{
    matcher::bible_matcher::BibleMatcher,
    segments::{
        autocomplete::{incomplete::IncompleteSegment, output::CompletionOutput},
        grammar::SegmentList,
        passage::Segments,
    },
};

pub struct InputAutoCompleter<'a> {
    matcher: &'a BibleMatcher,
}

impl<'a> InputAutoCompleter<'a> {
    pub fn new(matcher: &'a BibleMatcher) -> Self {
        Self { matcher }
    }

    /// - This assumes your cursor is at the end of the input
    pub fn suggest(&self, input: &str) -> Option<CompletionOutput> {
        let book_regex = self.matcher.data().books().book_regex();
        let cap = book_regex.captures_iter(input).last()?;
        let book_match = cap.get(1).unwrap();
        let book_id = self.matcher.data().books().search(book_match.as_str())?;

        // Same grammar as search: the last segment is the one still being typed
        let segments_input = &input[book_match.end()..];
        let list = SegmentList::parse(segments_input);
        if !segments_input[list.end()..].trim().is_empty() {
            // The cursor is not inside a reference
            return None;
        }
        let (complete, incomplete) = list.split_incomplete();
        let full_segments = Segments::from_nodes(complete);
        let incomplete_segment = IncompleteSegment::from_node(incomplete)?;

        let chapter_verses = self
            .matcher
            .data()
            .chapter_verses()
            .get_chapter_verses(&book_id)?;
        let suggestions = incomplete_segment.suggest(chapter_verses, full_segments.last())?;

        let start = book_match.start();
        Some(CompletionOutput::new(
            start,
            book_id,
            full_segments,
            suggestions,
        ))
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        matcher::bible_matcher::BibleMatcher, segments::autocomplete::input::InputAutoCompleter,
    };

    // use crate::{
    //     data::chapter_verses::BookChapterVerses,
    //     matcher::bible_matcher::BibleMatcher,
    //     segments::{
    //         autocomplete::{
    //             completer::SegmentAutoCompleter,
    //             input::{GROUP_MATCH, InputAutoCompleter},
    //         },
    //         segment::Segment,
    //     },
    // };
    //
    // #[test]
    // fn group_match() {
    //     let completer = SegmentAutoCompleter(BookChapterVerses::default());
    //     let matcher = BibleMatcher::default();
    //     let completer = InputAutoCompleter::new(&matcher, &completer);
    //     // completer.suggest("Genesis 1:1");
    //     let values = vec![
    //         "Genesis",
    //         "Genesis ",
    //         "Genesis 1",
    //         "Genesis 1:",
    //         "Genesis 1:1",
    //         "Genesis 1:1,",
    //         "Genesis 1:1-",
    //         "Genesis 1:1,2",
    //         "Genesis 1:1-2",
    //         "Genesis 1:1,2,",
    //         "Genesis 1:1-2:",
    //         "Genesis 1:1-2:3",
    //         "Genesis 1:1-2:3,",
    //     ];
    //     for val in values {
    //         let cap = GROUP_MATCH.captures_iter(val).next().unwrap();
    //         println!("Value: {:?}", val);
    //         println!("book: {:?}", cap.name("book").map(|c| c.as_str()));
    //         println!("valid: {:?}", cap.name("valid").map(|c| c.as_str()));
    //         println!(
    //             "incomplete: {:?}",
    //             cap.name("incomplete").map(|c| c.as_str())
    //         );
    //         // println!(
    //         //     "incomplete: {:?}",
    //         //     cap.name("incomplete")
    //         //         .map(|c| c.as_str().trim_end_matches(char::is_numeric))
    //         // );
    //         println!("----------------------");
    //     }
    // }
    //
    // #[test]
    // fn test_regex() {
    //     // let re = Regex::new(r#"(?<sc>\d*)(?<sv>:\d*)?((?:)(?<ec>-\d*)(?<ev>:\d*)?)?"#).unwrap();
    //     // let re = Regex::new(r#"(?<sc>\d*)(:(?<sv>\d*))?(-(?<ec>\d*)(:(?<ev>\d*))?)?"#).unwrap();
    //     let re = Regex::new(r#"^(?<sc>\d*)(:(?<sv>\d*))?(-(?<ec>\d*)(:(?<ev>\d*))?)?\d*"#).unwrap();
    //
    //     let mut values = vec!["", "1-", "1:", "1:1-", "1-2:", "1:1-2:"];
    //     // let values = vec!["9", "1-9", "1:9", "1:1-9", "1-2:9", "1:1-2:9"];
    //     values.extend(["9", "1-9", "1:9", "1:1-9", "1-2:9", "1:1-2:9"]);
    //     for v in values {
    //         if let Some(cap) = re.captures_iter(v).next() {
    //             // println!("Segment: {:?}", cap.get(0));
    //             println!("Segment: {:?}", v);
    //             println!("Start Chapter: {:?}", cap.name("sc").map(|c| c.as_str()));
    //             println!("Start Verse: {:?}", cap.name("sv").map(|c| c.as_str()));
    //             println!("End Chapter: {:?}", cap.name("ec").map(|c| c.as_str()));
    //             println!("End Verse: {:?}", cap.name("ev").map(|c| c.as_str()));
    //         }
    //         println!("----------------------");
    //     }
    // }
    //
    // #[test]
    // fn test_complete() {
    //     let completer = SegmentAutoCompleter(BookChapterVerses::default());
    //     let matcher = BibleMatcher::default();
    //     let completer = InputAutoCompleter::new(&matcher, &completer);
    //
    //     let mut values = vec!["", "1-", "1:", "1:1-", "1-2:", "1:1-2:"];
    //     values.extend(["9", "1-9", "1:9", "1:1-9", "1-2:9", "1:1-2:9"]);
    //     values.extend(["1:1-2:9,", "1:1-2:9,3", "1:1-2:9,3-", "1:1-2:9,3- hi"]);
    //     for v in values {
    //         completer.complete(&format!("Genesis {v}"));
    //     }
    //     // completer.complete(&format!("Genesis 1:1-2,3:"));
    // }
    /// Each suggestion as the full reference it would complete to
    fn suggestions(input: &str) -> Option<Vec<String>> {
        let matcher = BibleMatcher::default();
        let result = matcher.completer().suggest(input)?;
        Some(
            result
                .suggestions
                .into_iter()
                .map(|sug| result.segments.with_suggestion(sug).to_string())
                .collect(),
        )
    }

    #[test]
    fn suggests_from_the_shared_grammar() {
        // Nothing typed yet: every chapter of Genesis
        assert_eq!(suggestions("Genesis ").unwrap().len(), 50);
        // `1:` suggests the 31 verses of chapter 1
        let verses = suggestions("Gen. 1:").unwrap();
        assert_eq!(verses.len(), 31);
        assert_eq!(verses[0], "1:1");
        // `1:1-` suggests verse ends, then chapter ends
        let ranges = suggestions("Genesis 1:1-").unwrap();
        assert_eq!(ranges[0], "1:1-2");
        assert!(ranges.contains(&String::from("1:1-2:1")));
        // Complete segments before the cursor are kept
        let next = suggestions("Genesis 1:1, ").unwrap();
        assert_eq!(next[0], "1:1,2");
        // Same lexer as search, so Roman numerals and other dashes work here too
        assert_eq!(suggestions("Genesis i:").unwrap().len(), 31);
        assert_eq!(suggestions("Genesis 1:1–").unwrap()[0], "1:1-2");
    }

    #[test]
    fn no_suggestions_outside_a_reference() {
        assert!(suggestions("Genesis 1:1 and then").is_none());
        assert!(suggestions("no book here").is_none());
    }

    #[test]
    fn test_suggest() {
        let matcher = BibleMatcher::default();
        let completer = InputAutoCompleter::new(&matcher);

        let mut values = vec!["", "1-", "1:", "1:1-", "1-2:", "1:1-2:"];
        // values.extend(["9", "1-9", "1:9", "1:1-9", "1-2:9", "1:1-2:9"]);
        values.extend([
            "1:1-2:9,",
            "1:1-2:9,10",
            "1:1-2:9,10:",
            "1:1-2:9,10-",
            // "1:1-2:9,10-1",
            // "1:1-2:9,3", "1:1-2:9,3-", "1:1-2:9,3- hi"
        ]);
        let bk = "Genesis ";
        for v in values.into_iter().take(10) {
            let input = &format!("{bk}{v}");
            if let Some(result) = completer.suggest(input) {
                println!("{input}");
                let _total = result.suggestions.len();
                for sug in result.suggestions.into_iter() {
                    let segs = result.segments.with_suggestion(sug);
                    println!("{}{}", " ".repeat(bk.len()), segs);
                    // if idx > 5 {
                    //     println!("Total: {total}");
                    //     break;
                    // }
                }
                println!("{}", "-".repeat(80));
            }
        }
        // completer.complete(&format!("Genesis 1:1-2,3:"));
    }
}
