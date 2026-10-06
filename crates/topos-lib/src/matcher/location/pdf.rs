use std::str::Chars;

use mupdf::{Document, Rect, TextChar, TextPageOptions, pdf::PdfDocument};
use regex::Match;
use unicode_normalization::UnicodeNormalization;

use crate::matcher::{
    instance::BibleMatch,
    location::line_col::LineColLocation,
    matcher::{BibleMatcher, MatchResult, Matcher},
};

/**
- This is in the normal PDF coordinate space (origin is at the bottom left) used by PDF.js
- MuPDF coordinate space is at the top left: https://mupdf.readthedocs.io/en/latest/reference/common/coordinate-system.html

From PDF++
```ts
/**
 * [x1, y1, x2, y2], where [x1, y1] is the bottom-left corner and [x2, y2] is the top-right corner
 */
type Rect = [number, number, number, number];
```
*/
// TODO: Do I want to store page bounds?
#[derive(Clone, Debug)]
pub struct PDFRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl PDFRect {
    pub fn from_rects(page: Rect, line: Rect) -> Self {
        let x = line.x0;
        let y = page.y1 - line.y1;
        let w = line.x1 - line.x0;
        let h = line.y1 - line.y0;
        Self { x, y, w, h }
    }
}

fn merge_rects(rects: &[Rect]) -> Rect {
    let mut x0 = f32::MAX;
    let mut y0 = f32::MAX;
    let mut x1 = f32::MIN;
    let mut y1 = f32::MIN;

    for r in rects {
        x0 = x0.min(r.x0);
        y0 = y0.min(r.y0);
        x1 = x1.max(r.x1);
        y1 = y1.max(r.y1);
    }

    Rect { x0, y0, x1, y1 }
}

#[derive(thiserror::Error, Debug)]
pub enum PDFMatchError {
    #[error("Failed to read pages")]
    ReadPages,
    #[error("Failed to read Page {0}")]
    ReadPage(usize),
    #[error("Failed to read text on Page {0}")]
    ReadText(usize),
    #[error("Failed to page bounds for Page {0}")]
    PageBounds(usize),
}

#[derive(Clone, Debug)]
pub enum PDFLocation {
    Page(usize),
    // The Obsidian PDF++ selection (using PDF.js backend) is unstable
    // Selection,
    Rectangle { page: usize, rect: PDFRect },
    Rectangles { page: usize, rect: Vec<PDFRect> },
    Search { page: usize, query: String },
}

pub fn search_pdf_page2(
    matcher: &BibleMatcher,
    page_idx: usize,
    page: &mupdf::Page,
) -> MatchResult<Vec<BibleMatch<PDFLocation>>> {
    // ------------------------------------------------------------
    // 1. Extract normalized plain text for matching
    // ------------------------------------------------------------

    let raw_text = page
        .to_text()
        .map_err(|_| PDFMatchError::ReadText(page_idx))?;

    // Normalize to NFC to match glyph iteration
    let normalized_text: String = raw_text.nfc().collect();

    let mut matches: Vec<BibleMatch<LineColLocation>> =
        matcher.search::<LineColLocation>(&normalized_text)?;

    if matches.is_empty() {
        return Ok(vec![]);
    }

    // Matches must be sorted
    // matches.sort_by_key(|m| (m.location.start.line, m.location.start.column));

    // ------------------------------------------------------------
    // 2. Extract structured text (glyphs with geometry)
    // ------------------------------------------------------------
    let text_page = page
        // TODO: Should I preserve ligatures? What are they
        .to_text_page(
            TextPageOptions::empty()
                .union(TextPageOptions::PRESERVE_WHITESPACE)
                .union(TextPageOptions::PRESERVE_LIGATURES),
        )
        .map_err(|_| PDFMatchError::ReadText(page_idx))?;

    let page_bounds = page
        .bounds()
        .map_err(|_| PDFMatchError::PageBounds(page_idx))?;

    let mut results: Vec<BibleMatch<PDFLocation>> = Vec::with_capacity(matches.len());

    for result in &matches {
        // println!(
        //     "'{} {}'",
        //     matcher.data().books().get_name(result.psg.book).unwrap(),
        //     result.psg.segments
        // );
    }

    let mut m = PDFTextPageMatcher::new(&matches, &normalized_text, page_bounds, page_idx);

    // ------------------------------------------------------------
    // 3. Single-pass glyph iteration
    // ------------------------------------------------------------

    for block in text_page.blocks() {
        for line in block.lines() {
            for ch in line.chars() {
                let is_done = m.try_next_char(ch);
                if is_done {
                    return Ok(m.results);
                }
            }
            m.finish_line();
        }
    }
    // This is actually an error: I did not find them all
    // or maybe I have none?
    Ok(m.results)
}

#[derive(Debug)]
pub struct PDFTextPageMatcher<'a> {
    matches: &'a Vec<BibleMatch>,
    text: &'a str,
    page_bounds: mupdf::Rect,
    page_num: usize,
    match_idx: usize,
    char_idx: usize,
    // chars: Option<Chars<'a>>,
    line_rects: Vec<PDFRect>,
    char_rects: Vec<mupdf::Rect>,
    results: Vec<BibleMatch<PDFLocation>>,
}

impl<'a> PDFTextPageMatcher<'a> {
    pub fn new(
        matches: &'a Vec<BibleMatch>,
        text: &'a str,
        page_bounds: mupdf::Rect,
        page_num: usize,
    ) -> Self {
        Self {
            matches,
            text,
            page_bounds,
            page_num,
            match_idx: 0,
            char_idx: 0,
            // chars: matches[0].chars(),
            // chars: None,
            line_rects: vec![],
            char_rects: vec![],
            results: vec![],
        }
    }

    pub fn is_done(&self) -> bool {
        self.match_idx >= self.matches.len()
    }

    pub fn finish_line(&mut self) {
        if self.char_rects.is_empty() {
            return;
        }
        let merged = merge_rects(&self.char_rects);
        self.line_rects
            .push(PDFRect::from_rects(self.page_bounds, merged));
        self.char_rects.clear();
    }

    /// Returns `true` if all matches have been found
    // pub fn try_next_char(&mut self, ch: TextChar<'_>) -> bool {
    //     if let Some(c) = ch.char() {
    //         let current_char = self.current_char();
    //         if current_char.is_none() {
    //             dbg!(&self);
    //             dbg!(self.current_str());
    //             dbg!(c);
    //         }
    //         if current_char.unwrap() == c {
    //             let q = ch.quad();
    //             let did_last_char = self.char_found(c);
    //             let rect = mupdf::Rect {
    //                 x0: q.ll.x,
    //                 y0: q.ll.y,
    //                 x1: q.ur.x,
    //                 y1: q.ur.y,
    //             };
    //             if self.page_num == 16 {
    //                 dbg!(c, c.len_utf8(), &rect);
    //             }
    //             self.char_rects.push(rect);
    //             if did_last_char {
    //                 self.finish_match()
    //             } else {
    //                 false
    //             }
    //         } else {
    //             self.char_not_found();
    //             false
    //         }
    //     } else {
    //         false
    //     }
    // }
    pub fn try_next_char(&mut self, ch: TextChar<'_>) -> bool {
        if let Some(c) = ch.char() {
            let current_char = self.current_char();
            // if current_char.is_none() {
            //     dbg!(&self);
            //     dbg!(self.current_str());
            //     dbg!(c);
            // }
            let current_char = match current_char {
                Some(current_char) => current_char,
                // IDK
                None => return self.finish_match(),
            };
            if current_char == c {
                let q = ch.quad();
                // let ch = self.chars.unwrap().next();
                let did_last_char = self.char_found(c);
                let rect = mupdf::Rect {
                    x0: q.ll.x,
                    y0: q.ll.y,
                    x1: q.ur.x,
                    y1: q.ur.y,
                };
                // if self.page_num == 16 {
                //     dbg!(c, c.len_utf8(), &rect);
                // }
                self.char_rects.push(rect);
                if did_last_char {
                    self.finish_match()
                } else {
                    false
                }
            } else {
                self.char_not_found();
                false
            }
        } else {
            false
        }
    }

    /// Returns `true` if all matches have been found
    pub fn finish_match(&mut self) -> bool {
        if !self.char_rects.is_empty() {
            self.finish_line();
        }

        // println!("{:?}", self.current_str());
        let location = if self.line_rects.len() == 1 {
            PDFLocation::Rectangle {
                page: self.page_num,
                rect: self.line_rects.remove(0),
            }
        } else {
            PDFLocation::Rectangles {
                page: self.page_num,
                rect: std::mem::take(&mut self.line_rects),
            }
        };
        self.results.push(
            self.current_match()
                .expect("map_loc")
                .clone()
                .map_loc(|_| location),
        );
        // Reset
        self.match_idx += 1;
        self.char_idx = 0;
        // self.reset_chars();
        self.is_done()
    }

    // pub fn reset_chars(&'a mut self) {
    //     self.chars = Some(self.current_str().chars());
    // }

    pub fn current_match(&self) -> Option<&BibleMatch> {
        self.matches.get(self.match_idx)
    }

    pub fn current_loc(&self) -> LineColLocation {
        self.current_match().expect("current_loc").location
    }

    pub fn current_str(&self) -> &str {
        let loc = self.current_loc();
        &self.text[loc.bytes.start..loc.bytes.end]
    }

    pub fn char_not_found(&mut self) {
        self.char_idx = 0;
        // self.reset_chars();
        self.char_rects.clear();
        self.line_rects.clear();
    }

    /// True if did last character
    pub fn char_found(&mut self, c: char) -> bool {
        self.char_idx += 1;
        self.char_idx > self.max_char_idx()
        // self.chars.unwrap().peek
        // self.char_idx += c.len_utf8();
    }

    pub fn max_char_idx(&self) -> usize {
        let loc = self.current_loc();
        loc.bytes.end - loc.bytes.start
        // self.current_str()
        //     .chars()
        //     .map(|c| c.len_utf8())
        //     .sum::<usize>()
        // - 1
    }

    pub fn current_char(&mut self) -> Option<char> {
        // self.chars?.next()
        self.current_str().chars().nth(self.char_idx)
    }
}

impl Matcher for PDFLocation {
    type Input<'a> = &'a Document;

    fn search<'a>(
        matcher: &BibleMatcher,
        doc: Self::Input<'a>,
    ) -> MatchResult<Vec<BibleMatch<Self>>> {
        let mut matches = vec![];

        for (idx, page) in doc
            .pages()
            .map_err(|_| PDFMatchError::ReadPages)?
            .enumerate()
        {
            let page_num = idx + 1;
            let page = page.map_err(|_| PDFMatchError::ReadPage(page_num))?;
            let results = search_pdf_page2(matcher, page_num, &page)?;

            matches.extend(results);
        }

        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use mupdf::pdf::{PdfFilterOptions, PdfPage};

    use crate::error::AnyResult;

    use super::*;

    #[test]
    fn tdp_pdf() -> AnyResult<()> {
        let path = "/home/dgmastertemple/Dropbox/Apps/remotely-save/Dropbox Library/Books/PDF/The Dorean Principle - by Conley Owens.pdf";
        let doc = Document::open(path)?;
        let m = BibleMatcher::default();
        let results = m.search::<PDFLocation>(&doc)?;

        for result in &results {
            let name = format!(
                "'{} {}'",
                m.data().books().get_name(result.psg.book).unwrap(),
                result.psg.segments
            );
            match &result.location {
                PDFLocation::Page(_) => todo!(),
                PDFLocation::Rectangles { page, rect } => {
                    for rect in rect {
                        println!(
                            "[[The Dorean Principle - by Conley Owens.pdf#page={}&rect={},{},{},{}&color=yellow|p.{page} - {name}]]",
                            page,
                            rect.x,
                            rect.y,
                            rect.x + rect.w,
                            rect.y + rect.h,
                        )
                    }
                }
                PDFLocation::Search { page, query } => todo!(),
                PDFLocation::Rectangle { page, rect } => {
                    println!(
                        "[[The Dorean Principle - by Conley Owens.pdf#page={}&rect={},{},{},{}&color=yellow|p.{page} - {name}]]",
                        page,
                        rect.x,
                        rect.y,
                        rect.x + rect.w,
                        rect.y + rect.h,
                    )
                }
            }
        }

        Ok(())
    }
    // #[test]
    // fn add_annotation() {
    //     let path = "/home/dgmastertemple/Dropbox/Apps/remotely-save/Dropbox Library/Books/PDF/The Dorean Principle - by Conley Owens.pdf";
    //     let doc = Document::open(path)?;
    // }
    // fn create_an(p: &mut PdfPage) {
    //     use mupdf::pdf::PdfAnnotationType;
    //     let mut an = p.create_annotation(PdfAnnotationType::Text).unwrap();
    //     an.filter(PdfFilterOptions::default());
    //     let mut an = p.create_annotation(PdfAnnotationType::Highlight).unwrap();
    //     an.set_author("Person");
    // }
}
