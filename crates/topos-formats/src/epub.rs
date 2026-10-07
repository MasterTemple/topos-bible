//! EPUB: each reference's location as an EPUB CFI
//! ([EPUB Canonical Fragment Identifier](https://idpf.org/epub/linking/cfi/)), written the way the
//! [EPUB++](https://github.com/MasterTemple/epub-plus-plus) reader writes them, so its links open
//! straight to the reference.
//!
//! - The package steps are the `<spine>` and `<itemref>` elements' real positions in the OPF
//!   (`/6/14` for the 7th element in the 3rd), then `!` and the steps inside the content document,
//!   starting from `<body>` (`/4` when it follows `<head>`)
//! - Character offsets count UTF-16 code units (like the DOM), across every text node between two
//!   elements (comments don't split them)
//! - A reference is one range CFI, `epubcfi(parent,start,end)`, whose end is just after its last
//!   character
//! - `[id]` assertions are left out unless [`CfiOptions::assertions`] is set: they make a CFI more
//!   robust, but `[` and `]` break Obsidian wikilinks
//!
//! Only `<body>` is searched, without `script`, `style`, `template`, `noscript`, and the like, and
//! block elements (paragraphs, headings, list items, ...) are kept apart, so a reference never
//! joins text from two paragraphs.

use std::{
    cmp::Ordering,
    collections::HashMap,
    io::{Read, Seek},
    ops::Range,
    path::{Path, PathBuf},
};

use epub::doc::EpubDoc;
use roxmltree::{Document, Node, ParsingOptions};
use topos_bible::matcher::{BibleMatch, BibleMatcher};

use crate::{Format, FormatError};

#[derive(thiserror::Error, Debug)]
pub enum EPUBMatchError {
    #[error("Failed to read EPUB file: {0}")]
    Io(#[from] std::io::Error),
    #[error("EPUB structure or package error: {0}")]
    PackageError(String),
    #[error("XML parsing error: {0}")]
    XmlParse(String),
    #[error("Could not map byte index {0} to a valid CFI node")]
    CfiMapping(usize),
}

/// How CFIs are written
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CfiOptions {
    /// Add `[id]` assertions to steps whose element has an id (and to the spine item), like
    /// `/6/14[chapter-1]!/4/2[p12]/1:5`. More robust if the book changes, but the brackets break
    /// wikilinks, so EPUB++ leaves them out by default (and so does this)
    pub assertions: bool,
}

/// Where a reference is in an EPUB
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CfiLocation {
    /// The whole reference as one CFI: a range (`epubcfi(/6/14!/4/2,/1:0,/1:16)`), or a point when
    /// it is empty
    pub cfi: String,
    /// Where the reference starts, as a point CFI
    pub start: String,
    /// Just after where it ends, as a point CFI
    pub end: String,
    /// The position of its content document in the spine (counting only items in the manifest)
    pub spine_index: usize,
    /// The content document's path in the EPUB archive
    pub href: String,
    /// Its chapter: the first table-of-contents entry that points into its content document
    /// (EPUB++'s section title)
    pub chapter: Option<String>,
    /// Where it is in the book's text (each content document's text, with a blank line between
    /// them, like EPUB++'s `BookText` sections), in UTF-16 code units
    pub text_utf16: Range<usize>,
    /// The 1-based line of the book's text it starts on (a paragraph, or part of one at a `<br>`)
    pub line: usize,
    /// The 1-based UTF-16 column it starts at
    pub utf16_column: usize,
    /// The line it starts on
    pub line_text: String,
}

impl Format for CfiLocation {
    type Input<'a> = &'a Path;

    /// Writes CFIs without assertions; [`search_epub`] takes [`CfiOptions`]
    fn search(
        matcher: &BibleMatcher,
        input: Self::Input<'_>,
    ) -> Result<Vec<BibleMatch<Self>>, FormatError> {
        Ok(search_epub(matcher, input, CfiOptions::default())?)
    }
}

/// Finds the references in an EPUB file
pub fn search_epub(
    matcher: &BibleMatcher,
    path: &Path,
    options: CfiOptions,
) -> Result<Vec<BibleMatch<CfiLocation>>, EPUBMatchError> {
    let doc = EpubDoc::new(path).map_err(|e| EPUBMatchError::PackageError(e.to_string()))?;
    search_doc(matcher, doc, options)
}

/// Finds the references in an EPUB that is already in memory (or any reader)
pub fn search_epub_reader<R: Read + Seek>(
    matcher: &BibleMatcher,
    reader: R,
    options: CfiOptions,
) -> Result<Vec<BibleMatch<CfiLocation>>, EPUBMatchError> {
    let doc =
        EpubDoc::from_reader(reader).map_err(|e| EPUBMatchError::PackageError(e.to_string()))?;
    search_doc(matcher, doc, options)
}

fn search_doc<R: Read + Seek>(
    matcher: &BibleMatcher,
    mut doc: EpubDoc<R>,
    options: CfiOptions,
) -> Result<Vec<BibleMatch<CfiLocation>>, EPUBMatchError> {
    let root_file = doc.root_file.clone();
    let opf = doc.get_resource_str_by_path(&root_file).ok_or_else(|| {
        EPUBMatchError::PackageError(format!("missing package {}", root_file.display()))
    })?;
    let items = spine_items(&opf)?;
    let titles = toc_titles(&mut doc, &opf);

    let mut found = Vec::new();
    let mut spine_index = 0;
    // Where this content document's text starts in the book's text
    let (mut book_utf16, mut book_line) = (0, 0);
    for item in items {
        // Like EPUB++, itemrefs that aren't in the manifest aren't in the spine
        let Some(resource) = doc.resources.get(&item.idref) else {
            continue;
        };
        let index = spine_index;
        spine_index += 1;
        let path = resource.path.clone();
        let href = archive_path(&path);
        let Some(bytes) = doc.get_resource_by_path(&path) else {
            continue;
        };
        let xhtml = String::from_utf8_lossy(&bytes);
        // A content document that isn't well-formed XML (or isn't XHTML) is skipped
        let Ok(content) = ContentDocument::parse(&xhtml) else {
            continue;
        };
        let text = &content.text;
        let chapter = titles.get(&resolve_path("", &href)).cloned();
        let mut utf16 = Utf16Offsets::default();
        for m in matcher.search(text) {
            let bytes = m.location.bytes.start..m.location.bytes.end;
            let text_utf16 =
                book_utf16 + utf16.at(text, bytes.start)..book_utf16 + utf16.at(text, bytes.end);
            let line_start = text[..bytes.start].rfind('\n').map_or(0, |i| i + 1);
            let line_end = text[bytes.start..]
                .find('\n')
                .map_or(text.len(), |i| bytes.start + i);
            let (start, end) = content.points(&item.steps, bytes)?;
            let location = CfiLocation {
                cfi: range_cfi(&start, &end, options),
                start: point_cfi(&start, options),
                end: point_cfi(&end, options),
                spine_index: index,
                href: href.clone(),
                chapter: chapter.clone(),
                text_utf16,
                line: book_line + m.location.start.line,
                utf16_column: m.location.start.utf16_column,
                line_text: text[line_start..line_end].to_string(),
            };
            found.push(m.map_loc(|_| location));
        }
        book_utf16 += text.encode_utf16().count() + SEPARATOR.len();
        book_line += text.matches('\n').count() + 2;
    }
    Ok(found)
}

/// Converts increasing byte offsets in one text to UTF-16 offsets, counting each byte once
#[derive(Default)]
struct Utf16Offsets {
    byte: usize,
    utf16: usize,
}

impl Utf16Offsets {
    fn at(&mut self, text: &str, byte: usize) -> usize {
        if byte < self.byte {
            *self = Self::default();
        }
        self.utf16 += text[self.byte..byte].encode_utf16().count();
        self.byte = byte;
        self.utf16
    }
}

/// How a link to an EPUB is written
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkStyle {
    /// `[[Book.epub#epubcfi(...)|John 3:16]]`
    #[default]
    Wiki,
    /// `[John 3:16](Book.epub#epubcfi%28...%29)`
    Markdown,
}

/**
An EPUB++ link to a place in an EPUB, as its Obsidian plugin writes them

- `path` is the EPUB's path (forward slashes), `locator` a CFI (or any EPUB++ locator), and `label`
  the text shown
- Wikilinks can't hold `|`, `[`, `]`, `#`, or `^` in their label, so those become spaces (and CFIs
  with `[id]` assertions don't work in them)
- Markdown destinations are percent-encoded like `encodeURI`, plus `(`, `)`, and `:` (Obsidian
  ignores destinations with a raw `:`)

```
use topos_bible_formats::epub::{LinkStyle, epub_link};

let cfi = "epubcfi(/6/14!/4/2,/1:0,/1:16)";
assert_eq!(
    epub_link("Moby Dick.epub", cfi, "John 3:16", LinkStyle::Wiki),
    "[[Moby Dick.epub#epubcfi(/6/14!/4/2,/1:0,/1:16)|John 3:16]]"
);
assert_eq!(
    epub_link("Moby Dick.epub", cfi, "John 3:16", LinkStyle::Markdown),
    "[John 3:16](Moby%20Dick.epub#epubcfi%28/6/14!/4/2,/1%3A0,/1%3A16%29)"
);
```
*/
pub fn epub_link(path: &str, locator: &str, label: &str, style: LinkStyle) -> String {
    let one_line = label.split_whitespace().collect::<Vec<_>>().join(" ");
    match style {
        LinkStyle::Wiki => {
            let label = one_line
                .replace(['|', '[', ']', '#', '^'], " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let alias = if label.is_empty() {
                String::new()
            } else {
                format!("|{label}")
            };
            format!("[[{path}#{locator}{alias}]]")
        }
        LinkStyle::Markdown => {
            let label = one_line.replace('[', "\\[").replace(']', "\\]");
            let label = if label.is_empty() { path } else { &label };
            format!("[{label}]({}#{})", md_encode(path), md_encode(locator))
        }
    }
}

/// `encodeURI`, then `(`, `)`, and `:` (like EPUB++'s `mdEncode`)
fn md_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        let keep = byte.is_ascii_alphanumeric() || b"-_.!~*';/?@&=+$,#".contains(&byte);
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// CFI paths
// ---------------------------------------------------------------------------------------------

/// One step of a CFI path: an even index is an element, an odd one the text between elements
#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    index: usize,
    id: Option<String>,
    /// Preceded by `!` (the step into a content document)
    indirect: bool,
}

impl Step {
    fn new(index: usize, id: Option<&str>) -> Self {
        Self {
            index,
            id: id.filter(|id| !id.is_empty()).map(str::to_string),
            indirect: false,
        }
    }
}

/// An absolute CFI path and its character offset
#[derive(Clone, Debug, PartialEq, Eq)]
struct CfiPath {
    steps: Vec<Step>,
    offset: usize,
}

/// `^` before the characters that mean something in a CFI (EPUB++'s `escapeCfi`)
fn escape_cfi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '^' | '[' | ']' | '(' | ')' | ',' | ';' | '=') {
            out.push('^');
        }
        out.push(c);
    }
    out
}

fn write_steps(out: &mut String, steps: &[Step], options: CfiOptions) {
    for step in steps {
        if step.indirect {
            out.push('!');
        }
        out.push('/');
        out.push_str(&step.index.to_string());
        if let (true, Some(id)) = (options.assertions, &step.id) {
            out.push('[');
            out.push_str(&escape_cfi(id));
            out.push(']');
        }
    }
}

fn write_path(out: &mut String, steps: &[Step], offset: usize, options: CfiOptions) {
    write_steps(out, steps, options);
    out.push(':');
    out.push_str(&offset.to_string());
}

fn point_cfi(path: &CfiPath, options: CfiOptions) -> String {
    let mut out = String::from("epubcfi(");
    write_path(&mut out, &path.steps, path.offset, options);
    out.push(')');
    out
}

/// EPUB++'s `comparePaths`: by step indexes, then length, then offset
fn compare_paths(a: &CfiPath, b: &CfiPath) -> Ordering {
    let indexes = |p: &CfiPath| p.steps.iter().map(|s| s.index).collect::<Vec<_>>();
    let n = a.steps.len().min(b.steps.len());
    indexes(a)[..n]
        .cmp(&indexes(b)[..n])
        .then(a.steps.len().cmp(&b.steps.len()))
        .then(a.offset.cmp(&b.offset))
}

/// A range CFI with the shared steps factored out (EPUB++'s `makeRangeCfi`), or a point when
/// `start` and `end` are the same
fn range_cfi(start: &CfiPath, end: &CfiPath, options: CfiOptions) -> String {
    if compare_paths(start, end) == Ordering::Equal {
        return point_cfi(start, options);
    }
    let max = start.steps.len().min(end.steps.len()).saturating_sub(1);
    let shared = start
        .steps
        .iter()
        .zip(&end.steps)
        .take(max)
        .take_while(|(a, b)| a.index == b.index && a.indirect == b.indirect)
        .count();
    let mut out = String::from("epubcfi(");
    write_steps(&mut out, &start.steps[..shared], options);
    out.push(',');
    write_path(&mut out, &start.steps[shared..], start.offset, options);
    out.push(',');
    write_path(&mut out, &end.steps[shared..], end.offset, options);
    out.push(')');
    out
}

// ---------------------------------------------------------------------------------------------
// The package document
// ---------------------------------------------------------------------------------------------

/// An `<itemref>` and its steps in the package document (from `<package>`)
#[derive(Debug, PartialEq, Eq)]
struct SpineItem {
    idref: String,
    steps: Vec<Step>,
}

/// The real position of each `<itemref>` in the OPF, with its id (or else its idref) as the
/// assertion, like EPUB++
fn spine_items(opf: &str) -> Result<Vec<SpineItem>, EPUBMatchError> {
    with_xml(opf, |doc| {
        let package = doc.root_element();
        let Some((spine_index, spine)) = element_children(package)
            .enumerate()
            .find(|(_, el)| el.tag_name().name() == "spine")
        else {
            return vec![];
        };
        let spine_step = Step::new((spine_index + 1) * 2, spine.attribute("id"));
        element_children(spine)
            .enumerate()
            .filter(|(_, el)| el.tag_name().name() == "itemref")
            .map(|(idx, itemref)| {
                let idref = itemref.attribute("idref").unwrap_or_default().to_string();
                let id = itemref.attribute("id").unwrap_or(&idref);
                let steps = vec![spine_step.clone(), Step::new((idx + 1) * 2, Some(id))];
                SpineItem { idref, steps }
            })
            .collect()
    })
}

fn element_children<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    element_children(node).find(|el| el.tag_name().name() == name)
}

/// A path in the archive with forward slashes
fn archive_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

// ---------------------------------------------------------------------------------------------
// The table of contents
// ---------------------------------------------------------------------------------------------

/**
Each content document's chapter title, by its path ([`resolve_path`]'s form): the first entry in
the table of contents that points into it, like EPUB++'s section titles. The EPUB 3 navigation
document is read first, then the NCX.
*/
fn toc_titles<R: Read + Seek>(doc: &mut EpubDoc<R>, opf: &str) -> HashMap<String, String> {
    let nav = doc
        .resources
        .values()
        .find(|r| {
            r.properties
                .as_deref()
                .is_some_and(|p| p.split_whitespace().any(|p| p == "nav"))
        })
        .map(|r| r.path.clone());
    let toc = with_xml(opf, |xml| {
        child(xml.root_element(), "spine")
            .and_then(|spine| spine.attribute("toc"))
            .map(str::to_string)
    })
    .ok()
    .flatten();
    let ncx = toc
        .and_then(|id| doc.resources.get(&id))
        .or_else(|| {
            doc.resources
                .values()
                .find(|r| r.mime == "application/x-dtbncx+xml")
        })
        .map(|r| r.path.clone());
    let mut read = |path: Option<PathBuf>,
                    entries: fn(&Document, &str) -> Vec<(String, String)>| {
        let path = path?;
        let text = doc.get_resource_str_by_path(&path)?;
        let base = resolve_path("", &archive_path(&path));
        with_xml(&text, |xml| entries(xml, &base)).ok()
    };
    let mut entries = read(nav, nav_entries).unwrap_or_default();
    if entries.is_empty() {
        entries = read(ncx, ncx_entries).unwrap_or_default();
    }
    let mut titles = HashMap::new();
    for (href, label) in entries {
        let path = href.split('#').next().unwrap_or_default();
        if !path.is_empty() && !label.is_empty() {
            titles.entry(path.to_string()).or_insert(label);
        }
    }
    titles
}

/// The EPUB 3 navigation document's entries (href, label) in order: `<nav epub:type="toc">`
/// (else the first `<nav>`), its first `<ol>`, and the `<a>` (or `<span>`) of each `<li>`
fn nav_entries(doc: &Document, base: &str) -> Vec<(String, String)> {
    let navs: Vec<_> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "nav")
        .collect();
    let is_toc = |n: &&Node| {
        attribute_local(**n, "type").is_some_and(|t| t.split_whitespace().any(|t| t == "toc"))
    };
    let mut out = vec![];
    let nav = navs.iter().find(is_toc).or(navs.first());
    if let Some(ol) = nav.and_then(|nav| nav.descendants().find(|n| n.tag_name().name() == "ol")) {
        nav_list(ol, base, &mut out, 0);
    }
    out
}

fn nav_list(ol: Node, base: &str, out: &mut Vec<(String, String)>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for li in element_children(ol).filter(|n| n.tag_name().name() == "li") {
        let a = child(li, "a").or_else(|| child(li, "span"));
        let href = a
            .filter(|a| a.tag_name().name() == "a")
            .and_then(|a| a.attribute("href"))
            .filter(|href| !href.is_empty());
        let label = Some(a.map(text_of).unwrap_or_default())
            .filter(|label| !label.is_empty())
            .or_else(|| attribute_local(a.unwrap_or(li), "title").map(str::to_string))
            .unwrap_or_default();
        out.push((
            href.map(|h| resolve_path(base, h)).unwrap_or_default(),
            label,
        ));
        if let Some(sub) = child(li, "ol") {
            nav_list(sub, base, out, depth + 1);
        }
    }
}

/// The EPUB 2 NCX's entries (href, label) in order: each `<navPoint>` in `<navMap>`
fn ncx_entries(doc: &Document, base: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    if let Some(map) = doc.descendants().find(|n| n.tag_name().name() == "navMap") {
        nav_points(map, base, &mut out, 0);
    }
    out
}

fn nav_points(parent: Node, base: &str, out: &mut Vec<(String, String)>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for point in element_children(parent).filter(|n| n.tag_name().name() == "navPoint") {
        let label = child(point, "navLabel")
            .unwrap_or(point)
            .descendants()
            .find(|n| n.tag_name().name() == "text")
            .map(text_of)
            .unwrap_or_default();
        let src = child(point, "content")
            .and_then(|c| c.attribute("src"))
            .filter(|src| !src.is_empty());
        out.push((
            src.map(|s| resolve_path(base, s)).unwrap_or_default(),
            label,
        ));
        nav_points(point, base, out, depth + 1);
    }
}

/// An attribute by its local name, whatever its prefix (`epub:type`)
fn attribute_local<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.name() == name)
        .map(|a| a.value())
}

/// An element's text with whitespace collapsed
fn text_of(node: Node) -> String {
    let text: String = node
        .descendants()
        .filter(Node::is_text)
        .filter_map(|n| n.text())
        .collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `href` resolved against the folder of `base` (both archive paths), percent-decoded, with the
/// fragment kept (EPUB++'s `resolvePath`)
fn resolve_path(base: &str, href: &str) -> String {
    let scheme = href.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+.-".contains(&b))
    });
    if scheme {
        return href.to_string();
    }
    let (path, fragment) = match href.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (href, None),
    };
    let with_fragment = |path: &str| match fragment {
        Some(fragment) => format!("{path}#{fragment}"),
        None => path.to_string(),
    };
    if path.is_empty() {
        return with_fragment(base);
    }
    let decoded = percent_decode(path).unwrap_or_else(|| path.to_string());
    let mut parts: Vec<&str> = if decoded.starts_with('/') {
        vec![]
    } else {
        let folder = base.rsplit_once('/').map_or("", |(folder, _)| folder);
        folder.split('/').filter(|s| !s.is_empty()).collect()
    };
    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            segment => parts.push(segment),
        }
    }
    with_fragment(&parts.join("/"))
}

/// `decodeURIComponent`: `None` for a malformed escape or bytes that aren't UTF-8
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if byte == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(byte);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

// ---------------------------------------------------------------------------------------------
// Content documents
// ---------------------------------------------------------------------------------------------

/// Elements whose text isn't searched (EPUB++ skips the same ones)
const SKIP: [&str; 6] = ["script", "style", "template", "head", "title", "noscript"];

/// Elements that are kept apart from the text around them (EPUB++'s blocks; `br` is a line break)
const BLOCK: [&str; BLOCK_COUNT] = [
    "address",
    "article",
    "aside",
    "blockquote",
    "dd",
    "details",
    "dialog",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
    "img",
];
const BLOCK_COUNT: usize = 41;

/// Between blocks: a blank line, which ends a reference
const SEPARATOR: &str = "\n\n";

/// Deeper elements are left out (real books are nowhere near this)
const MAX_DEPTH: usize = 1024;

/// The whitespace that is collapsed (EPUB++'s: ASCII whitespace and the no-break space)
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\n' | '\t' | '\r' | '\x0c' | '\u{a0}')
}

/// Part of a text node copied unchanged into the searched text
#[derive(Debug)]
struct TextRun {
    /// Where it is in [`ContentDocument::text`]
    bytes: Range<usize>,
    /// Its text node's parent element, an index into [`ContentDocument::elements`] (`None` for
    /// the body)
    parent: Option<usize>,
    /// The odd step of the text between elements that it is part of
    chunk: usize,
    /// Its start in that text, in UTF-16 code units
    utf16_start: usize,
}

/// An element with text, as its step and its parent's index (`None` under `<body>`)
#[derive(Debug)]
struct Element {
    parent: Option<usize>,
    step: Step,
}

/// A content document's searchable text, and the text nodes each part of it came from
///
/// The text is like EPUB++'s `BookText` (its text for other tools to search): whitespace runs
/// (across elements too) are one space, blocks are kept apart by a blank line, and a `<br>` is a
/// line break.
#[derive(Debug, Default)]
struct ContentDocument {
    text: String,
    /// `<body>`'s step in `<html>`
    body_step: Step,
    /// `<body>` is the first
    elements: Vec<Element>,
    /// In document order, so ordered by their bytes too
    runs: Vec<TextRun>,
    /// Whitespace was seen since the last character
    pending_space: bool,
    /// A line break or blank line goes before the next character
    pending_break: Option<&'static str>,
    /// Where the last run would continue: its text node (by number) and its next UTF-16 offset in
    /// the chunk
    open_run: Option<(usize, usize)>,
    /// The text nodes seen, numbering them
    nodes: usize,
}

impl Default for Step {
    fn default() -> Self {
        Step::new(4, None)
    }
}

impl ContentDocument {
    fn parse(xhtml: &str) -> Result<Self, EPUBMatchError> {
        with_xml(xhtml, Self::from_doc)
    }

    fn from_doc(doc: &Document) -> Self {
        let html = doc.root_element();
        let body = element_children(html)
            .enumerate()
            .find(|(_, el)| el.tag_name().name() == "body");
        // EPUB++ shows the whole document as the body when there isn't one, at step 4
        let (body, body_step) = match body {
            Some((idx, body)) => (body, Step::new((idx + 1) * 2, body.attribute("id"))),
            None => (html, Step::new(4, None)),
        };
        let mut content = Self {
            body_step: Step {
                indirect: true,
                ..body_step
            },
            ..Self::default()
        };
        content.walk(body, None, 0);
        content
    }

    /// Adds an element's text: `this` is its index in `elements` (`None` for the body)
    fn walk(&mut self, node: Node, this: Option<usize>, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        // Text before the first element is chunk 1, after it chunk 3, ...
        let mut elements = 0;
        let mut utf16 = 0;
        for child in node.children() {
            if child.is_element() {
                elements += 1;
                utf16 = 0;
                let name = child.tag_name().name();
                if SKIP.contains(&name) {
                    continue;
                }
                // A line break, like EPUB++'s `BookText` (so both find the same references);
                // a reference can continue across it (`John 3:16-<br/>18`)
                if name == "br" {
                    if self.pending_break != Some(SEPARATOR) {
                        self.pending_break = Some("\n");
                    }
                    continue;
                }
                let block = BLOCK.contains(&name);
                if block {
                    self.pending_break = Some(SEPARATOR);
                }
                self.elements.push(Element {
                    parent: this,
                    step: Step::new(elements * 2, child.attribute("id")),
                });
                self.walk(child, Some(self.elements.len() - 1), depth + 1);
                if block {
                    self.pending_break = Some(SEPARATOR);
                }
            } else if child.is_text() {
                // Text and CDATA (comments and processing instructions don't count)
                let text = child.text().unwrap_or_default();
                self.add_text(text, this, elements * 2 + 1, utf16);
                utf16 += text.encode_utf16().count();
            }
        }
    }

    /// Adds a text node that starts at `utf16` in its chunk
    fn add_text(&mut self, text: &str, parent: Option<usize>, chunk: usize, utf16: usize) {
        let node = self.nodes;
        self.nodes += 1;
        // The offset of the single space before this character, when that's all the whitespace
        let mut lone_space = None;
        let mut offset = utf16;
        for c in text.chars() {
            let at = offset;
            offset += c.len_utf16();
            if is_space(c) {
                lone_space = (!self.pending_space && c == ' ').then_some(at);
                self.pending_space = true;
                continue;
            }
            if !self.text.is_empty() {
                if let Some(separator) = self.pending_break {
                    self.text.push_str(separator);
                    self.open_run = None;
                } else if self.pending_space {
                    self.text.push(' ');
                    // A single space in the same run is copied, so the run goes on
                    let same = lone_space.is_some_and(|space| self.open_run == Some((node, space)));
                    self.open_run = same.then_some((node, at));
                }
            }
            self.pending_break = None;
            self.pending_space = false;
            lone_space = None;
            if self.open_run != Some((node, at)) {
                let start = self.text.len();
                self.runs.push(TextRun {
                    bytes: start..start,
                    parent,
                    chunk,
                    utf16_start: at,
                });
            }
            self.text.push(c);
            if let Some(run) = self.runs.last_mut() {
                run.bytes.end = self.text.len();
            }
            self.open_run = Some((node, offset));
        }
    }

    /// The steps from `<body>` down to an element (`None` is the body)
    fn element_steps(&self, element: Option<usize>) -> Vec<Step> {
        let mut steps = vec![];
        let mut current = element;
        while let Some(element) = current.and_then(|idx| self.elements.get(idx)) {
            steps.push(element.step.clone());
            current = element.parent;
        }
        steps.reverse();
        steps
    }

    /// The text node and its chunk offset for a byte offset of the text: for a start, the
    /// character it starts at (the next node at a boundary); for an end, just after the character
    /// before it (the previous node). Whitespace that isn't in a run is skipped, like EPUB++.
    fn point(&self, byte: usize, is_end: bool) -> Result<(&TextRun, usize), EPUBMatchError> {
        let missing = || EPUBMatchError::CfiMapping(byte);
        let (run, byte) = if is_end {
            let idx = self.runs.partition_point(|r| r.bytes.start < byte);
            let run = idx
                .checked_sub(1)
                .and_then(|i| self.runs.get(i))
                .ok_or_else(missing)?;
            (run, byte.min(run.bytes.end))
        } else {
            let idx = self.runs.partition_point(|r| r.bytes.end <= byte);
            let run = self.runs.get(idx).ok_or_else(missing)?;
            (run, byte.max(run.bytes.start))
        };
        let before = self.text.get(run.bytes.start..byte).ok_or_else(missing)?;
        Ok((run, run.utf16_start + before.encode_utf16().count()))
    }

    /// The absolute start and end of a byte range of the text, after the spine item's steps
    fn points(
        &self,
        package_steps: &[Step],
        bytes: Range<usize>,
    ) -> Result<(CfiPath, CfiPath), EPUBMatchError> {
        let path = |byte: usize, is_end: bool| -> Result<CfiPath, EPUBMatchError> {
            let (run, offset) = self.point(byte, is_end)?;
            let mut steps = package_steps.to_vec();
            steps.push(self.body_step.clone());
            steps.extend(self.element_steps(run.parent));
            steps.push(Step::new(run.chunk, None));
            Ok(CfiPath { steps, offset })
        };
        let start = path(bytes.start, false)?;
        // An empty range is a point
        let end = if bytes.end > bytes.start {
            path(bytes.end, true)?
        } else {
            start.clone()
        };
        Ok((start, end))
    }
}

/// Parses XML (a DTD is allowed: most EPUB 2 documents have one) and reads it with `f`. HTML
/// entities that the DTD doesn't declare (like `&nbsp;`, which browsers know from XHTML's DTD)
/// are replaced with the characters they stand for, then it is parsed again.
fn with_xml<T>(text: &str, f: impl FnOnce(&Document) -> T) -> Result<T, EPUBMatchError> {
    let parse = |text| {
        let options = ParsingOptions {
            allow_dtd: true,
            ..ParsingOptions::default()
        };
        Document::parse_with_options(text, options)
    };
    let error = |e: roxmltree::Error| EPUBMatchError::XmlParse(e.to_string());
    match parse(text) {
        Ok(doc) => Ok(f(&doc)),
        Err(roxmltree::Error::UnknownEntityReference(..)) => {
            let replaced = replace_entities(text);
            parse(&replaced).map(|doc| f(&doc)).map_err(error)
        }
        Err(e) => Err(error(e)),
    }
}

/// The HTML entities that aren't in XML, as numeric references (the common ones in books)
const ENTITIES: [(&str, u32); 46] = [
    ("nbsp", 160),
    ("iexcl", 161),
    ("cent", 162),
    ("pound", 163),
    ("yen", 165),
    ("sect", 167),
    ("uml", 168),
    ("copy", 169),
    ("ordf", 170),
    ("laquo", 171),
    ("not", 172),
    ("shy", 173),
    ("reg", 174),
    ("deg", 176),
    ("plusmn", 177),
    ("sup2", 178),
    ("sup3", 179),
    ("acute", 180),
    ("micro", 181),
    ("para", 182),
    ("middot", 183),
    ("sup1", 185),
    ("ordm", 186),
    ("raquo", 187),
    ("frac14", 188),
    ("frac12", 189),
    ("frac34", 190),
    ("iquest", 191),
    ("ensp", 8194),
    ("emsp", 8195),
    ("thinsp", 8201),
    ("zwnj", 8204),
    ("zwj", 8205),
    ("ndash", 8211),
    ("mdash", 8212),
    ("lsquo", 8216),
    ("rsquo", 8217),
    ("sbquo", 8218),
    ("ldquo", 8220),
    ("rdquo", 8221),
    ("bdquo", 8222),
    ("dagger", 8224),
    ("Dagger", 8225),
    ("bull", 8226),
    ("hellip", 8230),
    ("trade", 8482),
];

/// Replaces the [`ENTITIES`] and [`LATIN1`] letters (`&eacute;`) with numeric references
fn replace_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let name = rest[1..]
            .find(';')
            .map(|end| &rest[1..end + 1])
            .filter(|name| name.len() <= 10 && name.bytes().all(|b| b.is_ascii_alphanumeric()));
        match name.and_then(entity) {
            Some(code) => {
                out.push_str(&format!("&#{code};"));
                rest = &rest[name.map_or(0, str::len) + 2..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<u32> {
    let named = ENTITIES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, code)| *code);
    named.or_else(|| {
        let idx = LATIN1.iter().position(|n| *n == name)?;
        u32::try_from(idx).ok().map(|idx| 192 + idx)
    })
}

/// U+00C0 to U+00FF
const LATIN1: [&str; 64] = [
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute",
    "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute",
    "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute",
    "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde",
    "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc",
    "uuml", "yacute", "thorn", "yuml",
];

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;

    const PLAIN: CfiOptions = CfiOptions { assertions: false };
    const ASSERTED: CfiOptions = CfiOptions { assertions: true };

    fn xhtml(body: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\">\n<head><title>John 1:1</title></head>\n<body id=\"top\">{body}</body>\n</html>"
        )
    }

    /// The range CFI of each reference in a content document at `/6/4[ch1]`
    fn cfis(body: &str, options: CfiOptions) -> Vec<String> {
        let content = ContentDocument::parse(&xhtml(body)).unwrap();
        let package = [Step::new(6, None), Step::new(4, Some("ch1"))];
        BibleMatcher::default()
            .search(&content.text)
            .into_iter()
            .map(|m| {
                let bytes = m.location.bytes.start..m.location.bytes.end;
                let (start, end) = content.points(&package, bytes).unwrap();
                range_cfi(&start, &end, options)
            })
            .collect()
    }

    #[test]
    fn one_paragraph() {
        let body = "<p id=\"p1\">See John 3:16 today.</p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2,/1:4,/1:13)"]);
        assert_eq!(
            cfis(body, ASSERTED),
            ["epubcfi(/6/4[ch1]!/4[top]/2[p1],/1:4,/1:13)"]
        );
    }

    #[test]
    fn head_is_not_searched() {
        assert!(cfis("<p>Nothing here.</p>", PLAIN).is_empty());
    }

    #[test]
    fn comments_do_not_split_text() {
        let body = "<p>Read <!-- a note --> John 3:16</p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2,/1:6,/1:15)"]);
        // CDATA counts like text
        let body = "<p>Read <![CDATA[it:]]> John 3:16</p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2,/1:9,/1:18)"]);
    }

    #[test]
    fn offsets_are_utf16() {
        let body = "<p>\u{1F600} \u{e9} John 3:16</p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2,/1:5,/1:14)"]);
    }

    #[test]
    fn second_paragraph() {
        let body = "\n<p>Intro.</p>\n<p>Then Romans 8:28.</p>\n";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/4,/1:5,/1:16)"]);
    }

    #[test]
    fn text_after_an_element() {
        let body = "<div>A <span>b</span> c <br/>Gen 1:1</div>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2,/5:0,/5:7)"]);
    }

    #[test]
    fn range_across_an_inline_element() {
        let body = "<p>Read <em id=\"e\">John</em> 3:16 now</p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2,/2/1:0,/3:5)"]);
        assert_eq!(
            cfis(body, ASSERTED),
            ["epubcfi(/6/4[ch1]!/4[top]/2,/2[e]/1:0,/3:5)"]
        );
    }

    #[test]
    fn whole_element_reference() {
        // Ends at the end of the em's text, not the start of the text after it
        let body = "<p>Read <em>John 3:16</em>, then more.</p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2/2,/1:0,/1:9)"]);
    }

    #[test]
    fn paragraphs_do_not_join() {
        assert!(cfis("<p>see John</p><p>3 apples</p>", PLAIN).is_empty());
        assert!(cfis("<div>see John</div>\n<div>3 apples</div>", PLAIN).is_empty());
    }

    #[test]
    fn scripts_are_skipped_but_counted() {
        let body = "<script>var John = '3:16';</script><p>Jude 5</p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/4,/1:0,/1:6)"]);
    }

    #[test]
    fn html_entities() {
        let doc = "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head/><body><p>&ldquo;&nbsp;Jn 3:16&rdquo; &eacute;</p></body></html>";
        let content = ContentDocument::parse(doc).unwrap();
        assert_eq!(content.text, "\u{201c} Jn 3:16\u{201d} \u{e9}");
    }

    #[test]
    fn whitespace_is_a_space() {
        // A no-break space is 2 bytes but 1 UTF-16 unit, like the space it becomes
        let body = "<p>(1&#160;John 4:11)</p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2,/1:1,/1:12)"]);
        let body = "<p>See John\n\n   3:16</p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2,/1:4,/1:17)"]);
        // Runs of whitespace are one space, even across elements
        let body = "<p>(1  John 4:11)</p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2,/1:1,/1:13)"]);
        let body = "<p>See John <em>\n 3:16</em></p>";
        assert_eq!(cfis(body, PLAIN), ["epubcfi(/6/4!/4/2,/1:4,/2/1:6)"]);
        // Single spaces don't split the text node's run
        let content = ContentDocument::parse(&xhtml("<p>a b c\n<b>d</b>  e</p>")).unwrap();
        assert_eq!(content.text, "a b c d e");
        assert_eq!(content.runs.len(), 3);
    }

    #[test]
    fn assertions_are_escaped() {
        assert_eq!(escape_cfi("a[1],(b);c=d^"), "a^[1^]^,^(b^)^;c^=d^^");
        let path = CfiPath {
            steps: vec![Step::new(6, None), Step::new(2, Some("x,y"))],
            offset: 0,
        };
        assert_eq!(point_cfi(&path, ASSERTED), "epubcfi(/6/2[x^,y]:0)");
        // The same start and end is a point
        assert_eq!(range_cfi(&path, &path, PLAIN), "epubcfi(/6/2:0)");
    }

    const OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">x</dc:identifier><dc:title>T</dc:title><dc:language>en</dc:language></metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1" href="text/one.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="text/two.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <!-- a comment -->
  <spine>
    <itemref idref="c1"/>
    <itemref idref="missing"/>
    <itemref idref="c2" id="second"/>
  </spine>
</package>"#;

    #[test]
    fn spine_steps_are_real_positions() {
        let items = spine_items(OPF).unwrap();
        let plain: Vec<_> = items
            .iter()
            .map(|item| {
                let mut out = String::new();
                write_steps(&mut out, &item.steps, ASSERTED);
                (item.idref.as_str(), out)
            })
            .collect();
        assert_eq!(
            plain,
            [
                ("c1", "/6/2[c1]".to_string()),
                ("missing", "/6/4[missing]".to_string()),
                ("c2", "/6/6[second]".to_string()),
            ]
        );
    }

    fn epub() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let files = [
            ("mimetype", "application/epub+zip".to_string()),
            (
                "META-INF/container.xml",
                r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_string(),
            ),
            ("OEBPS/content.opf", OPF.to_string()),
            (
                "OEBPS/nav.xhtml",
                xhtml(
                    "<nav><ol><li><span>Part</span><ol><li><a href=\"text/tw%6F.xhtml#p\">Chapter\n  Two</a></li></ol></li><li><a href=\"text/two.xhtml\">Later</a></li></ol></nav>",
                ),
            ),
            ("OEBPS/text/one.xhtml", xhtml("<p>First, Genesis 1:1.</p>")),
            (
                "OEBPS/text/two.xhtml",
                xhtml("<h1>Two</h1><p id=\"p\">Then Rev 22:21 and John 3:16-18</p>"),
            ),
        ];
        for (name, text) in files {
            zip.start_file(name, options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn searches_an_epub() {
        let matcher = BibleMatcher::default();
        let found = search_epub_reader(&matcher, Cursor::new(epub()), PLAIN).unwrap();
        let found: Vec<_> = found
            .iter()
            .map(|m| {
                let l = &m.location;
                (l.cfi.as_str(), l.spine_index, l.href.as_str())
            })
            .collect();
        assert_eq!(
            found,
            [
                ("epubcfi(/6/2!/4/2,/1:7,/1:18)", 0, "OEBPS/text/one.xhtml"),
                ("epubcfi(/6/6!/4/4,/1:5,/1:14)", 1, "OEBPS/text/two.xhtml"),
                ("epubcfi(/6/6!/4/4,/1:19,/1:31)", 1, "OEBPS/text/two.xhtml"),
            ]
        );

        let found = search_epub_reader(&matcher, Cursor::new(epub()), ASSERTED).unwrap();
        let location = &found[1].location;
        assert_eq!(
            location.cfi,
            "epubcfi(/6/6[second]!/4[top]/4[p],/1:5,/1:14)"
        );
        assert_eq!(location.start, "epubcfi(/6/6[second]!/4[top]/4[p]/1:5)");
        assert_eq!(location.end, "epubcfi(/6/6[second]!/4[top]/4[p]/1:14)");

        // Chapters come from the table of contents; positions run through the book's text
        let places: Vec<_> = found
            .iter()
            .map(|m| {
                let l = &m.location;
                let text = l.text_utf16.clone();
                (
                    l.chapter.as_deref(),
                    text,
                    l.line,
                    l.utf16_column,
                    l.line_text.as_str(),
                )
            })
            .collect();
        let paragraph = "Then Rev 22:21 and John 3:16-18";
        assert_eq!(
            places,
            [
                (None, 7..18, 1, 8, "First, Genesis 1:1."),
                (Some("Chapter Two"), 31..40, 5, 6, paragraph),
                (Some("Chapter Two"), 45..57, 5, 20, paragraph),
            ]
        );
    }

    #[test]
    fn resolves_paths_like_epub_plus_plus() {
        assert_eq!(
            resolve_path("OEBPS/nav.xhtml", "text/a%20b.xhtml#x"),
            "OEBPS/text/a b.xhtml#x"
        );
        assert_eq!(
            resolve_path("OEBPS/toc/nav.xhtml", "../a.xhtml"),
            "OEBPS/a.xhtml"
        );
        assert_eq!(resolve_path("OEBPS/nav.xhtml", "#x"), "OEBPS/nav.xhtml#x");
        assert_eq!(resolve_path("OEBPS/nav.xhtml", "/a.xhtml"), "a.xhtml");
        assert_eq!(resolve_path("", "a%zz.xhtml"), "a%zz.xhtml");
        assert_eq!(resolve_path("a/b", "https://x.org/y"), "https://x.org/y");
    }

    #[test]
    fn links() {
        let cfi = "epubcfi(/6/6!/4/4,/1:5,/1:14)";
        assert_eq!(
            epub_link(
                "Books/Café (2).epub",
                cfi,
                "Rev [22]:21",
                LinkStyle::Markdown
            ),
            "[Rev \\[22\\]:21](Books/Caf%C3%A9%20%282%29.epub#epubcfi%28/6/6!/4/4,/1%3A5,/1%3A14%29)"
        );
        assert_eq!(
            epub_link("Book.epub", cfi, "a|b [c]\n d", LinkStyle::Wiki),
            "[[Book.epub#epubcfi(/6/6!/4/4,/1:5,/1:14)|a b c d]]"
        );
        assert_eq!(
            epub_link("Book.epub", cfi, "", LinkStyle::Markdown),
            "[Book.epub](Book.epub#epubcfi%28/6/6!/4/4,/1%3A5,/1%3A14%29)"
        );
    }

    #[test]
    #[ignore = "needs a local EPUB; set TOPOS_EPUB to its path"]
    fn local_epub() -> Result<(), FormatError> {
        let path = std::env::var("TOPOS_EPUB").expect("TOPOS_EPUB is not set");
        let matcher = BibleMatcher::default();
        for m in CfiLocation::search(&matcher, Path::new(&path))? {
            println!("{}\t{}", m.psg.segments, m.location.cfi);
        }
        Ok(())
    }
}
