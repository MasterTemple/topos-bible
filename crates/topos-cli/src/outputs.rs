use std::time::Instant;

use clap::ValueEnum;
use topos_lib::{
    error::AnyResult, matcher::bible_matcher::BibleMatcher, segments::passage::Passage,
};

use crate::matches::PathMatches;

#[derive(Copy, Clone, Debug, Default, ValueEnum)]
pub enum OutputMode {
    #[value(alias = "c", help = "Count total matches")]
    Count,
    #[value(alias = "j", help = "Output matches as JSON")]
    JSON,
    #[default]
    #[value(alias = "t", help = "Output matches as a table")]
    Table,
    #[value(alias = "qf", help = "Output matches for the Neovim Quickfix List")]
    Quickfix,
}

impl OutputMode {
    /**
    TODO: this should not return an iterator of a struct, but an iterator of a type
    This type should implement [`OutputEntryFormat`]
    This is so that I can do different kinds of JSON outputs for example, based on the verbosity that the user requests (like context, ..)
    */
    pub fn write(
        &self,
        matcher: &BibleMatcher,
        results: impl Iterator<Item = AnyResult<PathMatches>>,
    ) {
        match self {
            OutputMode::Count => print_time(matcher, results),
            OutputMode::JSON => print_json(matcher, results),
            OutputMode::Table => print_table(matcher, results),
            OutputMode::Quickfix => print_qf_list(matcher, results),
        }
    }
}

/// BUG: The problem is that I am running the timer at the wrong spot, I think once the iterator is
/// created, it means all items have been sent
/// Here's an idea: pass the OutputMode to the matcher and call it on each iteration
fn print_time(_matcher: &BibleMatcher, results: impl Iterator<Item = AnyResult<PathMatches>>) {
    let start = Instant::now();
    let mut count = 0;
    // i think it skips this completely because there is code after it
    for PathMatches { path, matches } in results.filter_map(Result::ok) {
        let _path = path
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        count += matches.len();
    }
    println!("Matches: {}", count);
    println!("Elapsed: {}ms", start.elapsed().as_millis());
}

fn print_json(matcher: &BibleMatcher, results: impl Iterator<Item = AnyResult<PathMatches>>) {
    for PathMatches { path, matches } in results.filter_map(Result::ok) {
        let path = path
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        println!(r#"{{ "type": "start", "path": "{}" }}"#, path);
        for m in matches {
            let Passage { book, segments } = m.psg;

            let Some(book) = matcher.data().books().get_name(book) else {
                continue;
            };

            let _start = m.location.start;
            let _psg = format!("{} {}", book, segments);

            // println!("{}:{}:{}: {}", path, start.line, start.column, psg)
            println!(r#"{{ "type": "match" }}"#);
        }
        println!(r#"{{ "type": "end", "path": "{}" }}"#, path);
    }
}

fn print_qf_list(matcher: &BibleMatcher, results: impl Iterator<Item = AnyResult<PathMatches>>) {
    for PathMatches { path, matches } in results.filter_map(Result::ok) {
        let path = path
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        for m in matches {
            let Passage { book, segments } = m.psg;

            let Some(book) = matcher.data().books().get_name(book) else {
                continue;
            };

            let start = m.location.start;
            let psg = format!("{} {}", book, segments);

            println!("{}:{}:{}: {}", path, start.line, start.column, psg)
        }
    }
}

fn print_table(matcher: &BibleMatcher, results: impl Iterator<Item = AnyResult<PathMatches>>) {
    println!("| File | Line | Col | Verse |");
    println!("| ---- | ---- | --- | ----- |");
    for PathMatches { path, matches } in results.filter_map(Result::ok) {
        let path = path
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        for m in matches {
            let Passage { book, segments } = m.psg;

            let Some(book) = matcher.data().books().get_name(book) else {
                continue;
            };

            let start = m.location.start;

            println!(
                "| {} | {} | {} | {} {} |",
                path, start.line, start.column, book, segments
            )
        }
    }
}
