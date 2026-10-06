//! Properties that must hold for any input, shared by the randomized test and `fuzz/`.

use std::sync::LazyLock;

use crate::{
    matcher::{BibleMatcher, location::line_col::LineColLocation},
    segments::grammar::SegmentList,
};

static MATCHER: LazyLock<BibleMatcher> = LazyLock::new(BibleMatcher::default);

/// Panics if parsing, searching, or autocompleting `input` breaks an invariant
pub fn check(input: &str) {
    check_grammar(input);
    check_search(input);
    // Autocomplete only has to not panic
    let _ = MATCHER.completer().suggest(input);
}

fn check_span(input: &str, start: usize, end: usize) {
    assert!(
        start <= end && end <= input.len(),
        "{start}..{end} in {input:?}"
    );
    assert!(input.is_char_boundary(start) && input.is_char_boundary(end));
}

fn check_grammar(input: &str) {
    let list = SegmentList::parse(input);
    for (idx, node) in list.nodes.iter().enumerate() {
        let is_last = idx + 1 == list.nodes.len();
        assert!(
            is_last || node.is_complete(),
            "only the last node may dangle"
        );
        check_span(input, node.start.span.start, node.end());
        for part in node.parts() {
            check_span(input, part.delimiter.span.start, part.end());
        }
        assert!(node.complete_end() <= node.end());
    }
    if let Some(complete_end) = list.complete_end() {
        assert!(complete_end <= list.end());
    }
    check_span(input, 0, list.end());
}

fn check_search(input: &str) {
    for m in MATCHER.search::<LineColLocation>(input).unwrap() {
        let bytes = m.location.bytes;
        check_span(input, bytes.start, bytes.end);
        assert!(bytes.start < bytes.end);
        // A match ends on its last number, never on whitespace or a delimiter
        let last = input[..bytes.end].chars().next_back().unwrap();
        assert!(
            last.is_alphanumeric(),
            "match ends with {last:?} in {input:?}"
        );
        assert!(!m.psg.segments.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Characters that exercise every branch of the lexer and parser
    const ALPHABET: &[&str] = &[
        "0",
        "1",
        "2",
        "9",
        "25",
        "255",
        "256",
        "999",
        "1000",
        " ",
        "\n",
        ":",
        ".",
        ",",
        ";",
        "-",
        "–",
        "—",
        "⸺",
        "a",
        "b",
        "i",
        "v",
        "x",
        "l",
        "c",
        "iv",
        "xl",
        "John ",
        "1 John ",
        "Jude ",
        "Ps ",
        "Gen. ",
        "Matth. ",
        "Song of Songs ",
        "é",
        "日",
        "\u{301}",
        "and ",
        "I ",
        "-\n",
        "\u{AD}",
        "Ephe",
        "sians ",
        "ff",
        " f",
    ];

    /// A tiny xorshift generator, so the test is reproducible without dependencies
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 as usize
        }
    }

    #[test]
    fn random_inputs_hold_invariants() {
        let mut rng = Rng(0x5eed_1234_abcd_ef01);
        for _ in 0..20_000 {
            let len = rng.next() % 24;
            let input: String = (0..len)
                .map(|_| ALPHABET[rng.next() % ALPHABET.len()])
                .collect();
            check(&input);
        }
    }

    #[test]
    fn edge_cases_hold_invariants() {
        for input in [
            "",
            " ",
            "John",
            "John ",
            "John 255:255",
            "John 255-",
            "Jude 255",
            "Ps 150:255-",
            "Genesis 50:26-",
            "Genesis 1:31-",
            "John 3:16,",
            "John 21:25, ",
        ] {
            check(input);
        }
    }
}
