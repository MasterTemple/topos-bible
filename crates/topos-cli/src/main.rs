use clap::Parser;
use topos_lib::matcher::bible_matcher::BibleMatcher;

use crate::{args::Args, inputs::InputType};

pub mod args;
pub mod inputs;
pub mod matches;
pub mod outputs;

pub fn main() {
    let args = Args::parse();
    let input = InputType::new(args.input.clone());
    let output = args.mode;

    let matcher = BibleMatcher::try_from(args).unwrap();

    let results = input.search(matcher.clone());
    output.write(&matcher, results);
}
