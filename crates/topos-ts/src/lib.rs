use once_cell::sync::Lazy;
use topos_lib::{matcher::bible_matcher::BibleMatcher, segments::autocomplete::CompleteOptions};
use wasm_bindgen::prelude::*;

static BIBLE: Lazy<BibleMatcher> = Lazy::new(BibleMatcher::default);

#[wasm_bindgen]
pub fn search(input: &str) -> Vec<String> {
    let m = &*BIBLE;
    m.search(input)
        .iter()
        .map(|r| {
            let name = m.data().books().get_name(r.psg.book).unwrap();
            let segments = r.psg.segments.to_string();
            let start = r.location.start;
            format!("[{}:{}] {name} {segments}", start.line, start.utf16_column)
        })
        .collect()
}

#[wasm_bindgen]
pub fn autocomplete(input: &str) -> Vec<String> {
    let options = CompleteOptions::default();
    BIBLE
        .complete(input, input.len(), &options)
        .into_iter()
        .map(|completion| completion.label)
        .collect()
}
