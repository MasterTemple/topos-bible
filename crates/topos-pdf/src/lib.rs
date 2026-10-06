use mupdf::{Document, Page, Rect};
use topos_lib::{
    error::AnyResult,
    matcher::{bible_matcher::BibleMatcher, matches::ComplexFilter},
    segments::passage::Passage,
};

#[derive(Clone, Debug)]
pub enum PDFLocation {
    Page(usize),
    // The Obsidian PDF++ selection (using PDF.js backend) is unstable
    // Selection,
    Rectangle { page: usize, rect: PDFRect },
    Rectangles { page: usize, rect: Vec<PDFRect> },
    Search { page: usize, query: String },
}

impl PDFLocation {
    pub fn page(&self) -> usize {
        match self {
            PDFLocation::Page(page) => *page,
            PDFLocation::Rectangle { page, .. } => *page,
            PDFLocation::Rectangles { page, .. } => *page,
            PDFLocation::Search { page, .. } => *page,
        }
    }
}

/**
- This is in the normal PDF coordinate space
- MuPDF coordinate space: https://mupdf.readthedocs.io/en/latest/reference/common/coordinate-system.html
*/
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
        let w = line.x1;
        let h = page.y1 - line.y0;
        Self { x, y, w, h }
    }
}

#[derive(Clone, Debug)]
pub struct PDFBibleMatch {
    psg: Passage,
    location: PDFLocation,
}

#[derive(Clone, Debug)]
pub struct PDFBibleMatcher(BibleMatcher);

impl std::ops::Deref for PDFBibleMatcher {
    type Target = BibleMatcher;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Default for PDFBibleMatcher {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl PDFBibleMatcher {
    // TODO: For cross-page references I can use the first and last lines
    pub fn search(&self, doc: Document) -> AnyResult<Vec<PDFBibleMatch>> {
        let matcher = &self.0;

        let mut matches = vec![];

        for (idx, page) in doc.pages()?.enumerate() {
            let page = page?;
            let text = page.to_text()?;
            let results = matcher.search(&text);
            matches.extend(results.into_iter().map(|r| PDFBibleMatch {
                psg: r.psg,
                location: PDFLocation::Page(idx),
            }));
        }

        Ok(matches)
    }
}

/**
Obsidian PDF++:

```no_run
> [!PDF|yellow] [[01. O Perfect Redemption! - by Mike Riccardi - 117201843484205.pdf#page=3&selection=223,2,226,1&color=yellow|01. O Perfect Redemption! - by Mike Riccardi - 117201843484205, p.3]]
> > Hebrews 9:26
```

```no_run
Page: 3
```

*/
#[cfg(test)]
mod tests {
    use mupdf::{Document, TextPageOptions};
    use topos_lib::matcher::{bible_matcher::BibleMatcher, location::line_col::LineColLocation};

    use super::*;

    const PATH: &'static str = "/home/dgmastertemple/Dropbox/Apps/remotely-save/Dropbox Library/Papers/O Perfect Redemption/01. O Perfect Redemption! - by Mike Riccardi - 117201843484205.pdf";

    #[test]
    fn it_works() -> topos_lib::error::AnyResult<()> {
        let doc = Document::open(PATH)?;
        // let page = doc.load_page(3)?; // page 3?
        let page = doc.pages()?.nth(2).unwrap()?; // page 3
        //
        // page.to_display_list
        dbg!(page.bounds()?);
        let page_bounds = page.bounds()?;

        let text = page.to_text()?;
        let matcher = BibleMatcher::default();

        // println!("```\n{}\n```", &text);

        let matches = matcher.search::<LineColLocation>(&text).unwrap_or_default();
        // dbg!(&matches);
        let heb = &matches[0];
        dbg!(&heb);

        let text_page = page.to_text_page(TextPageOptions::BLOCK_TEXT)?;
        dbg!(page.search("Hebrews", 3));
        // dbg!(&text_page);
        let mut blocks = 0;
        let mut lines = 0;
        for block in text_page.blocks() {
            blocks += 1;
            // to understand bounds/rect: https://pymupdf.readthedocs.io/en/latest/rect.html
            // dbg!(&block.bounds());
            for line in block.lines() {
                lines += 1;
                // dbg!(&line.bounds());
                let line_text = line.chars().filter_map(|c| c.char()).collect::<String>();
                if line_text.contains("Hebrews") {
                    let PDFRect { x, y, w, h } = PDFRect::from_rects(page_bounds, line.bounds());
                    println!(
                        "![[test.pdf#page=3&rect={},{},{},{}&color=yellow|test, p.3]]",
                        x, y, w, h
                    );
                }
            }
        }

        Ok(())
    }

    #[test]
    fn find_all_references() -> topos_lib::error::AnyResult<()> {
        let path = PATH;
        let path = "/home/dgmastertemple/Dropbox/Apps/remotely-save/Dropbox Library/Books/PDF/The Dorean Principle - by Conley Owens.pdf";
        let doc = Document::open(path)?;
        let matcher = PDFBibleMatcher::default();
        let matches = matcher.search(doc);
        dbg!(&matches);
        println!("");
        println!("| Page | Reference |");
        println!("| ---- | --------- |");
        for m in matches? {
            let page = m.location.page() + 1;
            let book = matcher.data().books().get_name(m.psg.book).unwrap();
            let segments = m.psg.segments;

            println!("| {} | {} {} |", page, book, segments);
        }
        Ok(())
    }
}
