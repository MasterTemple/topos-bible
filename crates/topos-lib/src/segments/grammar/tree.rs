use std::iter::Peekable;

use crate::segments::grammar::lexer::{Lexer, Token};

/// Byte offsets into the parsed input (`start..end`)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberKind {
    Decimal,
    Roman,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Number {
    pub value: u8,
    pub kind: NumberKind,
    /// `b` in `28:18b`
    pub subverse: Option<char>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelimiterKind {
    /// `,` or `;`
    Segment,
    /// `:` or `.`
    Chapter,
    /// Various dashes
    Range,
}

impl DelimiterKind {
    const SEGMENT: &'static str = ",;";
    const CHAPTER: &'static str = ":.";
    /// hyphen, figure dash, en dash, em dash, minus sign, two-em dash
    const RANGE: &'static str = "-‒–—−⸺";

    pub fn from_char(c: char) -> Option<Self> {
        if Self::SEGMENT.contains(c) {
            Some(Self::Segment)
        } else if Self::CHAPTER.contains(c) {
            Some(Self::Chapter)
        } else if Self::RANGE.contains(c) {
            Some(Self::Range)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delimiter {
    pub kind: DelimiterKind,
    /// The character as written, so a formatter can keep or normalize it
    pub actual: char,
    pub span: Span,
}

/**
- A delimiter and the number after it, such as `:3` or `-4`
- `number` is [`None`] when the input ends (or stops being a reference) right after the delimiter,
  like the `-` in `1:1-`
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
    pub delimiter: Delimiter,
    pub number: Option<Number>,
}

impl Part {
    pub fn is_dangling(&self) -> bool {
        self.number.is_none()
    }

    pub fn end(&self) -> usize {
        match self.number {
            Some(number) => number.span.end,
            None => self.delimiter.span.end,
        }
    }
}

/**
One segment as written: `start(:start_verse)?(-end(:end_verse)?)?`

```text
1:2-3:4
│└┤└┤└┤
│ │ │ └ end_verse
│ │ └── end
│ └──── start_verse
└────── start
```

- Whether a number is a chapter or a verse depends on context, which is decided when resolving into
  [`Segments`](crate::segments::passage::Segments), not here
- Only the last part of the last node can be dangling, because parsing stops there
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentNode {
    /// The `,` or `;` before this segment ([`None`] for the first one)
    pub separator: Option<Delimiter>,
    pub start: Number,
    pub start_verse: Option<Part>,
    pub end: Option<Part>,
    pub end_verse: Option<Part>,
}

impl SegmentNode {
    pub fn parts(&self) -> impl Iterator<Item = &Part> {
        [&self.start_verse, &self.end, &self.end_verse]
            .into_iter()
            .flatten()
    }

    pub fn is_complete(&self) -> bool {
        self.parts().all(|p| !p.is_dangling())
    }

    /// The explicit start verse, if it was written and has a number
    pub fn start_verse_value(&self) -> Option<u8> {
        Some(self.start_verse?.number?.value)
    }

    /// The end number and the explicit end verse, if they were written and have numbers
    pub fn end_value(&self) -> Option<(u8, Option<u8>)> {
        let end = self.end?.number?.value;
        let end_verse = self.end_verse.and_then(|p| p.number).map(|n| n.value);
        Some((end, end_verse))
    }

    /// This node with only its first `keep` parts (`1:2-3:4` keeping 1 is `1:2`)
    pub fn truncated(&self, keep: usize) -> Self {
        let mut node = *self;
        let parts = [&mut node.start_verse, &mut node.end, &mut node.end_verse];
        for part in parts.into_iter().filter(|p| p.is_some()).skip(keep) {
            *part = None;
        }
        node
    }

    /// End of the last token, including a dangling delimiter
    pub fn end(&self) -> usize {
        self.parts().last().map_or(self.start.span.end, Part::end)
    }

    /// End of the last number that belongs to a complete part (what search should consume)
    pub fn complete_end(&self) -> usize {
        self.parts()
            .take_while(|p| !p.is_dangling())
            .last()
            .map_or(self.start.span.end, Part::end)
    }
}

/**
- The lossless parse of everything after a book name: every number and delimiter with its span
- This is the single grammar for segments:
  - search resolves the complete prefix (see [`SegmentList::complete_end`])
  - autocomplete uses [`SegmentList::split_incomplete`] to find what the user is still typing
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SegmentList {
    pub nodes: Vec<SegmentNode>,
    /// A separator with no segment after it yet (`1:1,`)
    pub trailing_separator: Option<Delimiter>,
}

impl SegmentList {
    /// This never fails: it takes as much as forms a reference and ignores the rest
    pub fn parse(input: &str) -> Self {
        let mut tokens = Lexer::new(input).peekable();
        let mut list = Self::default();

        let mut separator = None;
        while let Some(start) = take_number(&mut tokens) {
            let node = parse_node(&mut tokens, separator, start);
            list.nodes.push(node);
            if !node.is_complete() {
                return list;
            }
            separator = take_delimiter(&mut tokens, DelimiterKind::Segment);
            if separator.is_none() {
                return list;
            }
        }
        list.trailing_separator = separator;

        list
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// End of the last token, including dangling delimiters and a trailing separator
    pub fn end(&self) -> usize {
        match self.trailing_separator {
            Some(separator) => separator.span.end,
            None => self.nodes.last().map_or(0, SegmentNode::end),
        }
    }

    /// End of the complete reference, or [`None`] if nothing was parsed
    pub fn complete_end(&self) -> Option<usize> {
        self.nodes.last().map(SegmentNode::complete_end)
    }

    /**
    Splits off the segment that is still being typed (assuming the input ends at the cursor)
    - `John 1:1, 2:` → (`[1:1]`, `Some(2:)`)
    - `John 1:1, 2` → (`[1:1]`, `Some(2)`), since `2` could still become `25`
    - `John 1:1,` → (`[1:1]`, `None`): a new segment has not been started
    */
    pub fn split_incomplete(&self) -> (&[SegmentNode], Option<&SegmentNode>) {
        if self.trailing_separator.is_some() {
            return (&self.nodes, None);
        }
        match self.nodes.split_last() {
            Some((last, complete)) => (complete, Some(last)),
            None => (&[], None),
        }
    }
}

fn take_number(tokens: &mut Peekable<Lexer<'_>>) -> Option<Number> {
    match tokens.next_if(|t| matches!(t, Token::Number(_)))? {
        Token::Number(number) => Some(number),
        Token::Delimiter(_) => unreachable!(),
    }
}

fn take_delimiter(tokens: &mut Peekable<Lexer<'_>>, kind: DelimiterKind) -> Option<Delimiter> {
    match tokens.next_if(|t| matches!(t, Token::Delimiter(d) if d.kind == kind))? {
        Token::Delimiter(delimiter) => Some(delimiter),
        Token::Number(_) => unreachable!(),
    }
}

fn take_part(tokens: &mut Peekable<Lexer<'_>>, kind: DelimiterKind) -> Option<Part> {
    let delimiter = take_delimiter(tokens, kind)?;
    let number = take_number(tokens);
    Some(Part { delimiter, number })
}

fn parse_node(
    tokens: &mut Peekable<Lexer<'_>>,
    separator: Option<Delimiter>,
    start: Number,
) -> SegmentNode {
    let mut node = SegmentNode {
        separator,
        start,
        start_verse: None,
        end: None,
        end_verse: None,
    };

    node.start_verse = take_part(tokens, DelimiterKind::Chapter);
    if node.start_verse.is_some_and(|p| p.is_dangling()) {
        return node;
    }

    node.end = take_part(tokens, DelimiterKind::Range);
    if node.end.is_some_and(|p| !p.is_dangling()) {
        node.end_verse = take_part(tokens, DelimiterKind::Chapter);
    }

    node
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shows each node's numbers, with `_` for a dangling part
    fn shape(input: &str) -> Vec<String> {
        let list = SegmentList::parse(input);
        let mut out: Vec<String> = list
            .nodes
            .iter()
            .map(|node| {
                let mut s = node.start.value.to_string();
                for part in node.parts() {
                    s.push(part.delimiter.actual);
                    match part.number {
                        Some(n) => s.push_str(&n.value.to_string()),
                        None => s.push('_'),
                    }
                }
                s
            })
            .collect();
        if list.trailing_separator.is_some() {
            out.push(String::from(","));
        }
        out
    }

    #[test]
    fn complete_segments() {
        assert_eq!(shape("1"), ["1"]);
        assert_eq!(shape("1:2"), ["1:2"]);
        assert_eq!(shape("1-2"), ["1-2"]);
        assert_eq!(shape("1-2:3"), ["1-2:3"]);
        assert_eq!(shape("1:2-3"), ["1:2-3"]);
        assert_eq!(shape(" 1 : 1 - 2:   3"), ["1:1-2:3"]);
        assert_eq!(
            shape("1,2-4,5:1-3,5,7-9,12-6:6,7:7-8:8"),
            ["1", "2-4", "5:1-3", "5", "7-9", "12-6:6", "7:7-8:8"]
        );
        assert_eq!(shape(" 1 : 1a- 2:   3  ; 4:5-7"), ["1:1-2:3", "4:5-7"]);
    }

    #[test]
    fn partial_segments() {
        assert_eq!(shape(""), Vec::<String>::new());
        assert_eq!(shape("1:"), ["1:_"]);
        assert_eq!(shape("1-"), ["1-_"]);
        assert_eq!(shape("1:1-"), ["1:1-_"]);
        assert_eq!(shape("1-2:"), ["1-2:_"]);
        assert_eq!(shape("1:1-2:"), ["1:1-2:_"]);
        assert_eq!(shape("1:1,"), ["1:1", ","]);
        assert_eq!(shape("1:1, 2:"), ["1:1", "2:_"]);
    }

    #[test]
    fn stops_at_non_reference_text() {
        assert_eq!(shape("3:16 and 4"), ["3:16"]);
        assert_eq!(shape("3:16. For God"), ["3:16"]);
        assert_eq!(shape("1:2-3:4 5:6"), ["1:2-3:4"]);
    }

    #[test]
    fn complete_end_skips_dangling_parts() {
        let ends = |input: &str| {
            let list = SegmentList::parse(input);
            (list.complete_end(), list.end())
        };
        assert_eq!(ends("3:16"), (Some(4), 4));
        assert_eq!(ends("3:16, "), (Some(4), 5));
        assert_eq!(ends("3. For"), (Some(1), 2));
        assert_eq!(ends(" 1:1-2:"), (Some(6), 7));
        assert_eq!(ends("hello"), (None, 0));
    }

    #[test]
    fn split_incomplete() {
        let split = |input: &str| {
            let list = SegmentList::parse(input);
            let (complete, incomplete) = list.split_incomplete();
            (complete.len(), incomplete.map(|n| n.start.value))
        };
        assert_eq!(split(""), (0, None));
        assert_eq!(split("1"), (0, Some(1)));
        assert_eq!(split("1:1,"), (1, None));
        assert_eq!(split("1:1, 2:"), (1, Some(2)));
    }
}
