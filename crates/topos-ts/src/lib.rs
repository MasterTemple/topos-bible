use once_cell::sync::Lazy;
use topos_lib::matcher::{location::line_col::LineColLocation, matcher::BibleMatcher};
use wasm_bindgen::prelude::*;

static BIBLE: Lazy<BibleMatcher> = Lazy::new(|| BibleMatcher::default());

#[wasm_bindgen]
pub fn search(input: &str) -> Vec<String> {
    let m = &*BIBLE;
    m.search::<LineColLocation>(input)
        .unwrap_or_default()
        .iter()
        .map(|r| {
            let name = m.data().books().get_name(r.psg.book).unwrap();
            let segments = r.psg.segments.to_string();
            let start = r.location.start;
            format!("[{}:{}] {name} {segments}", start.line, start.column)
        })
        .collect()
}

#[wasm_bindgen]
pub fn autocomplete(input: &str) -> Option<Vec<String>> {
    let m = &*BIBLE;
    let comp = m.completer().suggest(input)?;
    let book = m.data().books().get_name(comp.book)?;
    Some(
        comp.suggestions
            .iter()
            .map(|sug| {
                let segs = comp.segments.with_suggestion(sug.clone());
                format!("{} {}", book, segs)
            })
            .collect(),
    )
}
