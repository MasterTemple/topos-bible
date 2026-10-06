//! Shell completions, computed by `topos` itself when Tab is pressed (clap_complete's dynamic
//! completions): options, every option's values (modes, formats, books, genres, testaments, named
//! queries), references for `-i`/`-o`/`--outside`, and paths.
//!
//! Set up once with `source <(COMPLETE=bash topos)` (or zsh, fish, elvish, powershell).

use std::ffi::OsStr;

use clap_complete::engine::CompletionCandidate;
use topos_bible::{
    data::books::BookId,
    matcher::BibleMatcher,
    segments::{
        autocomplete::{CompleteOptions, CompletionKind},
        formatter::{BookStyle, FormatOptions},
    },
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

/// One completion: the input with it applied, what a menu shows, and what it completes
#[derive(Debug, serde::Serialize)]
pub struct Suggestion {
    pub text: String,
    pub label: String,
    pub kind: &'static str,
}

/// A book's name in a style (`John`, `Jn`, `John` in OSIS)
pub fn book_in_style(matcher: &BibleMatcher, id: BookId, style: BookStyle) -> Option<String> {
    let books = matcher.data().books();
    match style {
        BookStyle::Name => books.get_name(id),
        BookStyle::Abbreviation => books.get_abbrev(id),
        BookStyle::Osis => books.get_osis(id),
    }
    .cloned()
}

/**
Completions for a partly typed reference, for Tab and `--complete`
- Nothing typed: every book (that the matcher's filters allow), in the format's book style
- Otherwise the engine's completions (books, then chapters, verses, and range ends), applied to
  the input, without ones that change nothing, and only those the filters keep
*/
pub fn suggestions(matcher: &BibleMatcher, text: &str, format: &FormatOptions) -> Vec<Suggestion> {
    let possible = matcher.possible_books();
    let allowed = |id: BookId| possible.as_ref().is_none_or(|books| books.contains(&id));
    let books = matcher.data().books();
    if text.trim().is_empty() {
        return books
            .ids()
            .filter(|id| allowed(*id))
            .filter_map(|id| {
                let text = book_in_style(matcher, id, format.book)?;
                let label = books.get_name(id).cloned().unwrap_or_else(|| text.clone());
                Some(Suggestion {
                    text,
                    label,
                    kind: "book",
                })
            })
            .collect();
    }
    let options = CompleteOptions {
        format: format.clone(),
        limit: None,
    };
    matcher
        .complete(text, text.len(), &options)
        .into_iter()
        .filter(|completion| match &completion.passage {
            Some(passage) => matcher.keeps(passage),
            None => books.search(&completion.label).is_none_or(allowed),
        })
        // The library leaves out completions that change nothing
        .map(|completion| {
            let mut full = text.to_string();
            full.replace_range(completion.edit.range.clone(), &completion.edit.text);
            let kind = match completion.kind {
                CompletionKind::Book => "book",
                CompletionKind::Chapter => "chapter",
                CompletionKind::Verse => "verse",
            };
            // OSIS is a reference format of its own (`John.3.16`), not a way to type one
            let osis = (format.book == BookStyle::Osis)
                .then(|| completion.passage.as_ref()?.to_osis(books))
                .flatten();
            let full = osis.unwrap_or_else(|| full.trim_end().to_string());
            Suggestion {
                text: full,
                label: completion.label,
                kind,
            }
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
    suggestions(&BibleMatcher::default(), &text, &FormatOptions::default())
        .into_iter()
        .map(|s| {
            // Book completions end with a space, ready for the chapter
            let text = if s.kind == "book" {
                format!("{} ", s.text)
            } else {
                s.text
            };
            candidate(value, text).help(Some(s.kind.into()))
        })
        .collect()
}
