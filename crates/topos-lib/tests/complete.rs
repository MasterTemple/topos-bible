//! The shared completion cases (`cases/complete.txt`), against the library

use topos_bible::{
    matcher::BibleMatcher,
    segments::{
        autocomplete::CompleteOptions,
        formatter::{BookStyle, FormatOptions},
    },
};

/// `(join_adjacent, abbreviate, input, expected first labels)` for each case
fn cases() -> Vec<(bool, bool, String, Vec<String>)> {
    include_str!("cases/complete.txt")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (input, expected) = line.split_once(" =>").expect("a case has `=>`");
            let join = input.starts_with("[join] ");
            let abbreviate = input.starts_with("[abbreviation] ");
            let input = input
                .trim_start_matches("[join] ")
                .trim_start_matches("[abbreviation] ");
            let expected = expected
                .split(" | ")
                .map(|label| label.trim().to_string())
                .filter(|label| !label.is_empty())
                .collect();
            (join, abbreviate, input.to_string(), expected)
        })
        .collect()
}

#[test]
fn completion_cases() {
    let matcher = BibleMatcher::default();
    let mut failures = vec![];
    for (join, abbreviate, input, expected) in cases() {
        let options = CompleteOptions {
            format: FormatOptions {
                join_adjacent: join,
                book: if abbreviate {
                    BookStyle::Abbreviation
                } else {
                    BookStyle::Name
                },
                ..FormatOptions::default()
            },
            limit: None,
        };
        let labels: Vec<String> = matcher
            .complete(&input, input.len(), &options)
            .into_iter()
            .map(|c| c.label)
            .collect();
        let ok = if expected.is_empty() {
            labels.is_empty()
        } else {
            labels.starts_with(&expected)
        };
        if !ok {
            failures.push(format!(
                "{input:?}: expected {expected:?}, got {:?}",
                &labels[..labels.len().min(4)]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
