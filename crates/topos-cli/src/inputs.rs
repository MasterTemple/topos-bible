use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;

use crossbeam_channel::{Receiver, unbounded};
use ignore::{WalkBuilder, WalkState};
use itertools::Either;
use topos_lib::error::AnyResult;
use topos_lib::matcher::matcher::BibleMatcher;

use crate::matches::PathMatches;

#[derive(Clone, Debug)]
pub enum InputType {
    Directory(PathBuf),
    File(PathBuf),
    TextInput(String),
}

impl InputType {
    pub fn new(input: Option<String>) -> Self {
        match input {
            Some(input) => {
                let path = PathBuf::from(&input);
                if path.is_file() {
                    Self::File(path)
                } else if path.is_dir() {
                    Self::Directory(path)
                } else {
                    Self::TextInput(input)
                }
            }
            None => {
                if !io::stdin().is_terminal() {
                    // If no positional argument is given and stdin is being piped, read from stdin
                    let mut buffer = String::new();
                    io::stdin().read_to_string(&mut buffer).unwrap();
                    Self::TextInput(buffer.trim_end().to_string())
                } else {
                    Self::default()
                }
            }
        }
    }

    pub fn search(self, matcher: BibleMatcher) -> impl Iterator<Item = AnyResult<PathMatches>> {
        match self {
            InputType::Directory(path) => Either::Left(handle_dir(path, matcher)),
            InputType::File(path) => {
                Either::Right(std::iter::once(
                    PathMatches::from_file(path, &matcher).map_err(Into::into),
                ))
                // Either::Right(std::iter::once(PathMatches::from_file(path, &matcher)))
                // Either::Right(std::iter::once(PathMatches::from_file(path, &matcher)))
            }
            InputType::TextInput(text) => {
                Either::Right(std::iter::once(Ok(PathMatches::from_text(text, &matcher))))
            }
        }
    }
}

impl Default for InputType {
    fn default() -> Self {
        Self::Directory(PathBuf::from("."))
    }
}

fn handle_dir(
    path: PathBuf,
    matcher: BibleMatcher,
) -> impl Iterator<Item = AnyResult<PathMatches>> {
    let walk = WalkBuilder::new(path);
    let receiver = run_multi_threaded_streaming(walk, &matcher);
    // BUG: Make it return a receiver, and have the single file/input return a receiver to iterate
    // over
    // I think it is actually collecting them all and then returning a full iterator
    receiver.into_iter().map(|r| Ok(r?))
}

fn run_multi_threaded_streaming<'scope>(
    walk: WalkBuilder,
    matcher: &'scope BibleMatcher,
) -> Receiver<Result<PathMatches, std::io::Error>> {
    let (sender, receiver) = unbounded();
    let walk = walk.build_parallel();

    std::thread::scope(|_s| {
        // let sender = sender.clone();
        // s.spawn(move || {
        walk.run(|| {
            let sender = sender.clone();
            Box::new(move |entry| {
                match entry {
                    Ok(entry) => {
                        if entry.path().is_dir() {
                            return WalkState::Continue;
                        }
                        let path = entry.path().to_path_buf();
                        let matches = PathMatches::from_file(path, matcher);
                        if sender.send(matches).is_err() {
                            return WalkState::Quit;
                        }
                    }
                    Err(err) => {
                        eprintln!("Error: {}", err);
                    }
                }
                WalkState::Continue
            })
        });
        // });
    });

    receiver
}
