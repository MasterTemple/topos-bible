use std::{process::ExitCode, sync::Arc};

use clap::{CommandFactory, Parser};

use crate::{
    args::{Args, OutputMode},
    cache::Cache,
    output::Printer,
    search::{CachedSearch, Input, Searcher, search},
};

mod args;
mod cache;
mod complete;
mod config;
mod output;
mod queries;
mod search;

/// Like ripgrep: 0 when something matched, 1 when nothing did, 2 on errors
fn main() -> ExitCode {
    // `COMPLETE=bash topos` prints the registration script; see complete::write_bash_registration
    if std::env::var_os("COMPLETE").is_some_and(|shell| shell == "bash")
        && std::env::args_os().len() == 1
    {
        return match complete::write_bash_registration(&mut std::io::stdout()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        };
    }
    // Answers the shell when Tab is pressed (COMPLETE=<shell> is set), then exits
    clap_complete::CompleteEnv::with_factory(Args::command).complete();
    let argv = match config::with_defaults(std::env::args_os().collect()).and_then(queries::expand)
    {
        Ok(argv) => argv,
        Err(err) => {
            eprintln!("topos: {err}");
            return ExitCode::from(2);
        }
    };
    let mut args = Args::parse_from(argv);
    if args.clear_cache {
        return match cache::clear() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("topos: could not delete the cache: {err}");
                ExitCode::from(2)
            }
        };
    }
    if args.list_queries {
        return match queries::load() {
            Ok(queries) => {
                for (name, query) in queries {
                    let query =
                        shlex::try_join(query.iter().map(String::as_str)).unwrap_or_default();
                    output::emit(format!("{name}\t{query}"));
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("topos: {err}");
                ExitCode::from(2)
            }
        };
    }
    let matcher = match args.matcher() {
        Ok(matcher) => matcher,
        Err(err) => {
            eprintln!("topos: {err}");
            return ExitCode::from(2);
        }
    };
    // Before reading the input, since these never search
    if let Some(text) = args.complete.take().or(args.list_books.then(String::new)) {
        let json = args.mode == OutputMode::Json;
        for suggestion in complete::suggestions(&matcher, &text, &args.format_options()) {
            if json {
                output::emit(serde_json::to_string(&suggestion).unwrap_or_default());
            } else {
                output::emit(suggestion.text);
            }
        }
        return ExitCode::SUCCESS;
    }
    let input = match Input::new(std::mem::take(&mut args.paths), args.text.take()) {
        Ok(input) => input,
        Err(err) => {
            eprintln!("topos: {err}");
            return ExitCode::from(2);
        }
    };

    if args.epub_links.is_some() && matches!(input, Input::Text(_)) {
        eprintln!("topos: --epub-links searches EPUB files, not text");
        return ExitCode::from(2);
    }
    let roots = match &input {
        Input::Paths(paths) => paths.clone(),
        Input::Text(_) => vec![],
    };
    let mut printer = Printer::new(&args, matcher.data().clone(), roots);
    let (before, after) = args.context_lines();
    let cached = args
        .cache
        .then(|| Cache::open(&args.fingerprint()))
        .flatten()
        .map(|cache| CachedSearch::new(cache, &matcher));
    let walk = match args.walk_options() {
        Ok(walk) => walk,
        Err(err) => {
            eprintln!("topos: {err}");
            return ExitCode::from(2);
        }
    };
    let searcher = Arc::new(Searcher {
        matcher,
        // --files never searches, so it never needs the cache
        cached: cached.filter(|_| !args.files),
        needs_text: before + after > 0 || printer.needs_text(),
        walk,
        binary: args.search_binary(),
        list_only: args.files,
        cfi: args.cfi_options(),
    });
    let results = search(searcher.clone(), input);
    let mut files: Vec<_> = vec![];
    let (mut found, mut failed) = (false, false);
    for result in results {
        match result {
            Ok(file) if args.sort => files.push(file),
            Ok(file) => found |= printer.file(&file),
            Err((path, err)) => {
                failed = true;
                match path {
                    Some(path) => eprintln!("topos: {}: {err}", path.display()),
                    None => eprintln!("topos: {err}"),
                }
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    for file in &files {
        found |= printer.file(file);
    }
    printer.finish();
    if let Some(cached) = &searcher.cached {
        cached.cache.finish();
    }

    if failed {
        ExitCode::from(2)
    } else if found {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
