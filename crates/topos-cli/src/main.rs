use std::{process::ExitCode, sync::Arc};

use clap::Parser;

use crate::{
    args::Args,
    output::Printer,
    search::{Input, search},
};

mod args;
mod output;
mod search;

/// Like ripgrep: 0 when something matched, 1 when nothing did, 2 on errors
fn main() -> ExitCode {
    let mut args = Args::parse();
    let matcher = match args.matcher() {
        Ok(matcher) => Arc::new(matcher),
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
    let results = search(matcher, input);
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

    if failed {
        ExitCode::from(2)
    } else if found {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
