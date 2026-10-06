//! Runs every case in `tests/cases/search.txt` and reports all failures at once.

use topos_lib::matcher::{BibleMatcher, location::line_col::LineColLocation};

struct Case<'a> {
    line: usize,
    input: String,
    expected: Vec<&'a str>,
    known_failure: bool,
}

fn cases(text: &str) -> Vec<Case<'_>> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|(idx, l)| {
            let (known_failure, l) = match l.strip_prefix('?') {
                Some(rest) => (true, rest),
                None => (false, l),
            };
            let (input, expected) = l
                .split_once(" =>")
                .unwrap_or_else(|| panic!("line {}: missing ` =>`", idx + 1));
            let expected = expected
                .split(" | ")
                .map(str::trim)
                .filter(|e| !e.is_empty())
                .collect();
            Case {
                line: idx + 1,
                // `\n` in a case stands for a line break
                input: input.replace("\\n", "\n"),
                expected,
                known_failure,
            }
        })
        .collect()
}

#[test]
fn search_cases() {
    let matcher = BibleMatcher::default();
    let mut failures = vec![];
    let mut fixed = vec![];
    let mut known = 0;

    for case in cases(include_str!("cases/search.txt")) {
        let actual: Vec<String> = matcher
            .search::<LineColLocation>(&case.input)
            .unwrap()
            .into_iter()
            .map(|m| {
                let book = matcher.data().books().get_name(m.psg.book).unwrap();
                format!("{book} {}", m.psg.segments)
            })
            .collect();
        let passed = actual == case.expected;
        let report = format!(
            "line {}: {:?}\n    expected {:?}\n    actual   {:?}",
            case.line, case.input, case.expected, actual
        );
        match (passed, case.known_failure) {
            (false, false) => failures.push(report),
            (true, true) => fixed.push(report),
            (false, true) => known += 1,
            (true, false) => {}
        }
    }

    if known > 0 {
        eprintln!("{known} known failures (marked with `?`)");
    }
    assert!(
        failures.is_empty() && fixed.is_empty(),
        "\nFailures:\n{}\n\nNow passing, remove the `?`:\n{}\n",
        failures.join("\n"),
        fixed.join("\n")
    );
}
