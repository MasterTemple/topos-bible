use crate::matcher::{
    bible_matcher::{BibleMatcher, MatchResult},
    instance::BibleMatch,
    location::line_col::LineColLocation,
};

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

use roxmltree::Node;

#[derive(Debug)]
struct TextNodeSpan {
    byte_start: usize,
    byte_end: usize,
    cfi_path: String,
}

pub struct EpubDocumentMapper {
    base_cfi: String,
    pub plain_text: String,
    spans: Vec<TextNodeSpan>,
}

impl EpubDocumentMapper {
    pub fn new(base_cfi: String, xml_content: &str) -> Result<Self, EPUBMatchError> {
        // Parse the XHTML document
        let doc = roxmltree::Document::parse(xml_content)
            .map_err(|e| EPUBMatchError::XmlParse(e.to_string()))?;

        let mut plain_text = String::new();
        let mut spans = Vec::new();

        // Start walking from the root element.
        // In XHTML, the root <html> is the first element, so its local step is /2
        if let Some(root) = doc.root_element().into() {
            Self::walk_xml(root, "", &mut plain_text, &mut spans);
        }

        Ok(Self {
            base_cfi,
            plain_text,
            spans,
        })
    }

    fn walk_xml(
        node: Node,
        current_cfi: &str,
        plain_text: &mut String,
        spans: &mut Vec<TextNodeSpan>,
    ) {
        let mut element_count = 0;
        let mut text_node_index = 1;

        for child in node.children() {
            if child.is_element() {
                element_count += 1;
                let step = element_count * 2;

                // If the element has an ID, CFI encourages including it in brackets for robustness
                let step_str = match child.attribute("id") {
                    Some(id) => format!("{}[{}]", step, id),
                    None => step.to_string(),
                };

                let child_cfi = format!("{}/{}", current_cfi, step_str);

                // Recurse into the element
                Self::walk_xml(child, &child_cfi, plain_text, spans);

                // The next text node will sit after this element
                text_node_index = step + 1;
            } else if child.is_text() {
                let text = child.text().unwrap_or("");
                if !text.is_empty() {
                    let byte_start = plain_text.len();
                    plain_text.push_str(text);
                    let byte_end = plain_text.len();

                    spans.push(TextNodeSpan {
                        byte_start,
                        byte_end,
                        cfi_path: format!("{}/{}", current_cfi, text_node_index),
                    });
                }
            }
        }
    }

    pub fn byte_index_to_cfi(&self, byte_index: usize) -> Result<String, EPUBMatchError> {
        let span = self
            .spans
            .iter()
            .find(|s| byte_index >= s.byte_start && byte_index < s.byte_end)
            .ok_or(EPUBMatchError::CfiMapping(byte_index))?;

        let bytes_into_span = byte_index - span.byte_start;
        let text_up_to_match =
            &self.plain_text[span.byte_start..(span.byte_start + bytes_into_span)];

        // CFI counts character offsets, not byte offsets!
        let chars_into_span = text_up_to_match.chars().count();

        // Assemble: epubcfi( /spine_path ! /local_path : char_offset )
        Ok(format!(
            "epubcfi({}!{}:{})",
            self.base_cfi, span.cfi_path, chars_into_span
        ))
    }
}

use epub::doc::EpubDoc;

#[derive(Clone, Debug)]
pub struct CfiLocation {
    pub start_cfi: String,
    pub end_cfi: String,
}

pub fn search_epub(
    epub_path: &str,
    matcher: &BibleMatcher,
) -> MatchResult<Vec<BibleMatch<CfiLocation>>> {
    let mut all_matches = Vec::new();

    let mut doc =
        EpubDoc::new(epub_path).map_err(|e| EPUBMatchError::PackageError(e.to_string()))?;

    // The spine holds the reading order of the EPUB
    let spines = doc.spine.clone();

    for (index, spine) in spines.iter().enumerate() {
        // Construct the base CFI for the package/spine.
        // Assuming `<spine>` is element /6 in the OPF, and items are 2, 4, 6...
        let spine_step = (index + 1) * 2;
        let base_cfi = format!("/6/{}[{}]", spine_step, spine.idref);

        // Get the raw XHTML for this chapter/section
        let (html_bytes, mime) = doc.get_resource(&spine.idref).ok_or_else(|| {
            EPUBMatchError::PackageError(format!("Missing resource {}", spine.idref))
        })?;

        dbg!(&spine.idref);
        dbg!(&mime);
        let html_content = String::from_utf8_lossy(&html_bytes);

        // Map the document
        // let mapper = EpubDocumentMapper::new(base_cfi, &html_content)?;
        let Ok(mapper) = EpubDocumentMapper::new(base_cfi, &html_content) else {
            continue;
        };

        // Run your existing matcher
        let local_results = matcher.search::<LineColLocation>(&mapper.plain_text)?;

        // Convert the ByteIndexes to CFIs
        for bible_match in local_results {
            let start_cfi = mapper.byte_index_to_cfi(bible_match.location.bytes.start)?;

            // Use (end - 1) if your matcher's end index is exclusive, so we map to the last valid char
            let end_cfi =
                mapper.byte_index_to_cfi(bible_match.location.bytes.end.saturating_sub(1))?;

            let cfi_match = bible_match.map_loc(|_| CfiLocation { start_cfi, end_cfi });

            all_matches.push(cfi_match);
        }
    }

    Ok(all_matches)
}

#[test]
#[ignore = "needs a local EPUB; set TOPOS_EPUB to its path"]
fn epub_tdp() -> MatchResult<()> {
    let path = std::env::var("TOPOS_EPUB").expect("TOPOS_EPUB is not set");
    let matcher = BibleMatcher::default();
    dbg!(search_epub(&path, &matcher)?);
    Ok(())
}
