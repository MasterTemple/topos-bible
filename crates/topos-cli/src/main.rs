use std::{process::ExitCode, sync::Arc};

use clap::{CommandFactory, Parser};

use crate::{
    args::Args,
    cache::Cache,
    output::Printer,
    search::{Input, Searcher, search},
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
    if args.list_queries {
        return match queries::load() {
            Ok(queries) => {
                for (name, query) in queries {
                    let query =
                        shlex::try_join(query.iter().map(String::as_str)).unwrap_or_default();
                    println!("{name}\t{query}");
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
    let input = match Input::new(std::mem::take(&mut args.paths), args.text.take()) {
        Ok(input) => input,
        Err(err) => {
            eprintln!("topos: {err}");
            return ExitCode::from(2);
        }
    };

    let mut printer = Printer::new(&args, matcher.data().clone());
    let (before, after) = args.context_lines();
    let searcher = Arc::new(Searcher {
        matcher,
        cache: args
            .cache
            .then(|| Cache::open(&args.fingerprint()))
            .flatten(),
        needs_text: before + after > 0 || printer.needs_text(),
        extensions: args
            .extensions
            .iter()
            .map(|ext| ext.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|ext| !ext.is_empty())
            .collect(),
    });
    let results = search(searcher.clone(), input);
    let mut files: Vec<_> = vec![];
    let (mut found, mut failed) = (false, false);
    for result in results {
        match result {
            Ok(file) if args.sort => files.push(file),
            Ok(file) => {
                found |= !file.hits.is_empty();
                printer.file(&file);
            }
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
        found |= !file.hits.is_empty();
        printer.file(file);
    }
    printer.finish();
    if let Some(cache) = &searcher.cache
        && let Err(err) = cache.save()
    {
        eprintln!("topos: could not save the cache: {err}");
    }

    if failed {
        ExitCode::from(2)
    } else if found {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
