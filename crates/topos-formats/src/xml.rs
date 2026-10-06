use std::ops::Range;

use roxmltree::{Document, Node};
use topos_lib::matcher::{BibleMatch, BibleMatcher};

use crate::{Format, FormatError};

/// Where a match is in an XML document
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XMLLocation {
    /// Path to the deepest element containing the whole match, like `/book/chapter[2]/p[5]`
    /// (positions are 1-based among siblings with the same name, and only written when needed)
    pub path: String,
    /// Byte range within that element's text, where its text nodes are joined with `\n`
    pub range: Range<usize>,
}

#[derive(thiserror::Error, Debug)]
pub enum XMLMatchError {
    #[error("invalid XML: {0}")]
    Parse(#[from] roxmltree::Error),
}

/// The document's text nodes joined with `\n`, and where each one starts
struct XMLText<'a, 'input> {
    text: String,
    nodes: Vec<(usize, Node<'a, 'input>)>,
}

impl<'a, 'input> XMLText<'a, 'input> {
    fn new(root: Node<'a, 'input>) -> Self {
        let mut text = String::new();
        let mut nodes = vec![];
        for node in root.descendants().filter(Node::is_text) {
            if !text.is_empty() {
                // A line break keeps text from separate elements apart, without ending a reference
                text.push('\n');
            }
            nodes.push((text.len(), node));
            text.push_str(node.text().unwrap_or_default());
        }
        Self { text, nodes }
    }

    fn node_at(&self, offset: usize) -> Node<'a, 'input> {
        let idx = self.nodes.partition_point(|(start, _)| *start <= offset);
        self.nodes[idx.saturating_sub(1)].1
    }

    /// Where an element's text starts in the joined text
    fn element_start(&self, element: Node) -> usize {
        self.nodes
            .iter()
            .find(|(_, node)| node.ancestors().any(|a| a == element))
            .map_or(0, |(start, _)| *start)
    }
}

impl Format for XMLLocation {
    type Input<'a> = &'a str;

    fn search(
        matcher: &BibleMatcher,
        input: Self::Input<'_>,
    ) -> Result<Vec<BibleMatch<Self>>, FormatError> {
        let doc = Document::parse(input).map_err(XMLMatchError::from)?;
        let text = XMLText::new(doc.root());
        Ok(matcher
            .search(&text.text)
            .into_iter()
            .map(|m| {
                let bytes = m.location.bytes;
                let first = text.node_at(bytes.start);
                let last = text.node_at(bytes.end.saturating_sub(1));
                let element = common_element(first, last);
                let start = text.element_start(element);
                m.map_loc(|_| XMLLocation {
                    path: path(element),
                    range: bytes.start - start..bytes.end - start,
                })
            })
            .collect())
    }
}

/// The deepest element that contains both nodes
fn common_element<'a, 'input>(a: Node<'a, 'input>, b: Node<'a, 'input>) -> Node<'a, 'input> {
    a.ancestors()
        .filter(Node::is_element)
        .find(|candidate| b.ancestors().any(|other| other == *candidate))
        .unwrap_or_else(|| a.document().root_element())
}

fn path(element: Node) -> String {
    let mut parts: Vec<String> = element
        .ancestors()
        .filter(Node::is_element)
        .map(|node| {
            let name = node.tag_name().name();
            let same_name = |n: &Node| n.is_element() && n.tag_name().name() == name;
            let siblings = node
                .parent()
                .map_or(1, |p| p.children().filter(same_name).count());
            if siblings > 1 {
                // `prev_siblings` starts with the node itself, so this is 1-based
                let position = node.prev_siblings().filter(same_name).count();
                format!("{name}[{position}]")
            } else {
                name.to_string()
            }
        })
        .collect();
    parts.reverse();
    format!("/{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_elements() {
        let xml =
            "<doc><p>intro</p><p>See <i>John</i> 3:16 &amp; Rom 8:28</p><note>Ps 23</note></doc>";
        let matches = XMLLocation::search(&BibleMatcher::default(), xml).unwrap();
        let locations: Vec<_> = matches
            .iter()
            .map(|m| (m.location.path.as_str(), m.location.range.clone()))
            .collect();
        // `John` is in <i>, but the reference continues into <p>; text is "See \nJohn\n 3:16 & Rom 8:28"
        assert_eq!(
            locations,
            [
                ("/doc/p[2]", 5..15),
                ("/doc/p[2]", 18..26),
                ("/doc/note", 0..5)
            ]
        );
    }
}
