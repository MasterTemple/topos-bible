//! Shell completions, computed by `topos` itself when Tab is pressed (clap_complete's dynamic
//! completions): options, every option's values (modes, formats, books, genres, testaments, named
//! queries), references for `-i`/`-o`/`--outside`, and paths.
//!
//! Set up once with `source <(COMPLETE=bash topos)` (or zsh, fish, elvish, powershell).

use std::ffi::OsStr;

use clap_complete::engine::CompletionCandidate;
use topos_lib::{
    matcher::BibleMatcher,
    segments::autocomplete::{CompleteOptions, CompletionKind},
};

/// What is typed, without the shell's quoting: bash passes the word as typed, so `"1 Co` or
/// `1\ Co` should both mean `1 Co`
fn current(value: &OsStr) -> String {
    let raw = value.to_string_lossy();
    if let Some(quoted) = raw.strip_prefix(['"', '\'']) {
        return quoted.to_string();
    }
    let mut text = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        text.push(if c == '\\' {
            chars.next().unwrap_or(c)
        } else {
            c
        });
    }
    text
}

/// A candidate (bash quotes it for the word as typed: see [`write_bash_registration`])
fn candidate(_value: &OsStr, text: impl Into<String>) -> CompletionCandidate {
    CompletionCandidate::new(text.into())
}

/// clap_complete's bash registration, plus `-o filenames`: readline then escapes candidates for
/// an unquoted word (`Song\ of\ Solomon`) and leaves them alone inside quotes, which topos
/// can't do itself because bash passes the word without its opening quote
pub fn write_bash_registration(out: &mut impl std::io::Write) -> std::io::Result<()> {
    use clap_complete::env::{Bash, EnvCompleter};
    let bin = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "topos".into());
    let mut script = vec![];
    Bash.write_registration("COMPLETE", "topos", "topos", &bin, &mut script)?;
    let script = String::from_utf8_lossy(&script).replacen(
        "local IFS=$'\\013'",
        "local IFS=$'\\013'\n    compopt -o filenames 2> /dev/null",
        1,
    );
    out.write_all(script.as_bytes())
}

fn starts_with(name: &str, prefix: &str) -> bool {
    name.to_lowercase().starts_with(&prefix.to_lowercase())
}

/// Book names, matching any name or abbreviation (`jn` offers John)
pub fn books(value: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current(value);
    let matcher = BibleMatcher::default();
    let books = matcher.data().books();
    books
        .ids()
        .filter_map(|id| {
            let name = books.get_name(id)?;
            let abbrev = books.get_abbrev(id).cloned().unwrap_or_default();
            let matches = starts_with(name, &prefix)
                || starts_with(&abbrev, &prefix)
                || books
                    .iter_keys_and_ids()
                    .any(|(key, other)| *other == id && starts_with(key, &prefix));
            matches.then(|| candidate(value, name).help(Some(abbrev.into())))
        })
        .collect()
}

pub fn genres(value: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current(value);
    let matcher = BibleMatcher::default();
    matcher
        .data()
        .genres()
        .iter()
        .filter(|genre| starts_with(genre.name(), &prefix))
        .map(|genre| {
            let count = genre.books().len();
            candidate(value, genre.name()).help(Some(format!("{count} books").into()))
        })
        .collect()
}

pub fn testaments(value: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current(value);
    [("old", "Old Testament"), ("new", "New Testament")]
        .into_iter()
        .filter(|(name, _)| starts_with(name, &prefix))
        .map(|(name, help)| candidate(value, name).help(Some(help.into())))
        .collect()
}

/// Named queries from queries.toml, with what each expands to
pub fn queries(value: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current(value);
    let Ok(queries) = crate::queries::load() else {
        return vec![];
    };
    queries
        .into_iter()
        .filter(|(name, _)| starts_with(name, &prefix))
        .map(|(name, args)| {
            let help = shlex::try_join(args.iter().map(String::as_str)).unwrap_or_default();
            candidate(value, name).help(Some(help.into()))
        })
        .collect()
}

/// References, with the same autocomplete as the editor plugins: books, then chapters, verses,
/// and range ends (`John 3:` offers every verse of John 3)
pub fn passages(value: &OsStr) -> Vec<CompletionCandidate> {
    let text = current(value);
    if text.trim().is_empty() {
        return books(value);
    }
    let matcher = BibleMatcher::default();
    let options = CompleteOptions::default();
    let completions = matcher.complete(&text, text.len(), &options);
    // The engine also offers the typed reference in full (`Rom 8` → `Romans 8`)
    completions
        .into_iter()
        .filter_map(|completion| {
            let mut full = text.clone();
            full.replace_range(completion.edit.range.clone(), &completion.edit.text);
            let help = match completion.kind {
                CompletionKind::Book => "book",
                CompletionKind::Chapter => "chapter",
                CompletionKind::Verse => "verse",
            };
            (full != text.trim_end()).then(|| candidate(value, full).help(Some(help.into())))
        })
        .collect()
}
