use std::ops::Range;

use mupdf::{Document, Page, Rect, TextPageFlags};
use topos_bible::matcher::{BibleMatch, BibleMatcher};

use crate::{Format, FormatError};

/// Where a match is in a PDF: its page and one rectangle per line it covers
#[derive(Clone, Debug, PartialEq)]
pub struct PDFLocation {
    /// 1-based page number
    pub page: usize,
    pub rects: Vec<PDFRect>,
}

/**
- A rectangle in PDF coordinates (origin at the bottom left, as PDF.js and Obsidian PDF++ use)
- MuPDF puts the origin at the top left:
  <https://mupdf.readthedocs.io/en/latest/reference/common/coordinate-system.html>
*/
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PDFRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl PDFRect {
    /// Converts a MuPDF rectangle on a page with these bounds
    pub fn new(page: Rect, rect: Rect) -> Self {
        Self {
            x: rect.x0,
            y: page.y1 - rect.y1,
            w: rect.x1 - rect.x0,
            h: rect.y1 - rect.y0,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PDFMatchError {
    #[error("failed to read the pages: {0}")]
    ReadPages(String),
    #[error("failed to read page {page}: {error}")]
    ReadPage { page: usize, error: String },
}

/// A character of the page text, where it is in that text, and where it is drawn
struct Glyph {
    offset: usize,
    line: usize,
    rect: Rect,
}

/// The text of a page, built from its glyphs so every byte can be mapped back to a rectangle
struct PageText {
    text: String,
    glyphs: Vec<Glyph>,
}

impl PageText {
    fn new(page: &Page) -> Result<Self, mupdf::Error> {
        let text_page = page.to_text_page(TextPageFlags::PRESERVE_WHITESPACE)?;
        let mut text = String::new();
        let mut glyphs = vec![];
        let mut line_idx = 0;
        for block in text_page.blocks() {
            for line in block.lines() {
                for ch in line.chars() {
                    let Some(c) = ch.char() else { continue };
                    glyphs.push(Glyph {
                        offset: text.len(),
                        line: line_idx,
                        rect: Rect::from(ch.quad()),
                    });
                    text.push(c);
                }
                text.push('\n');
                line_idx += 1;
            }
            // Blocks are paragraphs, and a blank line ends a reference
            text.push('\n');
        }
        Ok(Self { text, glyphs })
    }

    /// One rectangle per line that the byte range covers
    fn rects(&self, bytes: Range<usize>) -> Vec<Rect> {
        let first = self.glyphs.partition_point(|g| g.offset < bytes.start);
        let mut rects: Vec<(usize, Rect)> = vec![];
        for glyph in self.glyphs[first..]
            .iter()
            .take_while(|g| g.offset < bytes.end)
        {
            match rects.last_mut() {
                Some((line, rect)) if *line == glyph.line => *rect = rect.union(&glyph.rect),
                _ => rects.push((glyph.line, glyph.rect)),
            }
        }
        rects.into_iter().map(|(_, rect)| rect).collect()
    }
}

impl Format for PDFLocation {
    type Input<'a> = &'a Document;

    /// Pages are searched one at a time, so a reference split across pages is not found
    fn search(
        matcher: &BibleMatcher,
        doc: Self::Input<'_>,
    ) -> Result<Vec<BibleMatch<Self>>, FormatError> {
        let pages = doc
            .pages()
            .map_err(|e| PDFMatchError::ReadPages(e.to_string()))?;
        let mut matches = vec![];
        for (idx, page) in pages.enumerate() {
            let page_num = idx + 1;
            let read_error = |error: mupdf::Error| PDFMatchError::ReadPage {
                page: page_num,
                error: error.to_string(),
            };
            let page = page.map_err(read_error)?;
            let bounds = page.bounds().map_err(read_error)?;
            let text = PageText::new(&page).map_err(read_error)?;
            matches.extend(matcher.search(&text.text).into_iter().map(|m| {
                let bytes = m.location.bytes;
                let rects = text
                    .rects(bytes.start..bytes.end)
                    .into_iter()
                    .map(|rect| PDFRect::new(bounds, rect))
                    .collect();
                m.map_loc(|_| PDFLocation {
                    page: page_num,
                    rects,
                })
            }));
        }
        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal one-page PDF with a line of text per entry, 20 points apart
    fn pdf(lines: &[&str]) -> Vec<u8> {
        let mut content = String::from("BT /F1 12 Tf 72 720 Td 14 TL\n");
        for line in lines {
            content.push_str(&format!("({line}) Tj T*\n"));
        }
        content.push_str("ET");
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = String::from("%PDF-1.4\n");
        let mut offsets = vec![];
        for (idx, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.push_str(&format!("{} 0 obj\n{object}\nendobj\n", idx + 1));
        }
        let xref = out.len();
        out.push_str(&format!(
            "xref\n0 {}\n0000000000 65535 f \n",
            objects.len() + 1
        ));
        for offset in offsets {
            out.push_str(&format!("{offset:010} 00000 n \n"));
        }
        out.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        ));
        out.into_bytes()
    }

    #[test]
    fn locates_references_on_pages() {
        let bytes = pdf(&["Read John 3:16 and Romans", "8:28 today."]);
        let doc = Document::from_bytes(&bytes, "application/pdf").unwrap();
        let matcher = BibleMatcher::default();
        let matches = PDFLocation::search(&matcher, &doc).unwrap();

        let refs: Vec<_> = matches.iter().map(|m| m.psg.segments.to_string()).collect();
        assert_eq!(refs, ["3:16", "8:28"]);
        assert!(matches.iter().all(|m| m.location.page == 1));
        // `John 3:16` is on one line; `Romans 8:28` wraps onto a second line
        assert_eq!(matches[0].location.rects.len(), 1);
        assert_eq!(matches[1].location.rects.len(), 2);
        // The first line is near the top of the page (y is measured from the bottom)
        let rect = matches[0].location.rects[0];
        assert!(rect.y > 700.0 && rect.w > 0.0 && rect.h > 0.0, "{rect:?}");
    }
}
