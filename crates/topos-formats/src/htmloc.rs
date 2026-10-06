//! Text fragments (`#:~:text=`) for positions in HTML, and positions for text fragments: the
//! location the HTML format reports. Formerly the `htmloc` crate
//! (<https://github.com/MasterTemple/htmloc>).

use line_col::LineColLookup;
use regex::Regex;
use urlencoding::{decode, encode};

const MAX_EXACT_MATCH_LENGTH: usize = 300;
const MIN_CONTEXT_WORDS: usize = 3;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ByteIndex {
    pub start: usize,
    pub end: usize,
}

impl<'h> From<regex::Match<'h>> for ByteIndex {
    fn from(value: regex::Match<'h>) -> Self {
        ByteIndex::new(value.start(), value.end())
    }
}
impl ByteIndex {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,   // 1-indexed
    pub column: usize, // 1-indexed
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub start: Position,
    pub end: Position,
    pub bytes: ByteIndex,
}

impl Selection {
    pub fn new(start: Position, end: Position, bytes: ByteIndex) -> Self {
        Self { start, end, bytes }
    }
}

#[derive(Debug, Clone, Default)]
pub struct GenerateOptions {
    pub min_prefix_words: usize,
    pub min_suffix_words: usize,
}

impl GenerateOptions {
    pub fn new(min_prefix_words: usize, min_suffix_words: usize) -> Self {
        Self {
            min_prefix_words,
            min_suffix_words,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFragment {
    pub prefix: Option<String>,
    pub text_start: String,
    pub text_end: Option<String>,
    pub suffix: Option<String>,
}

impl TextFragment {
    /// Serializes the fragment into the standard URL hash format
    pub fn to_hash_string(&self) -> String {
        let mut parts = Vec::new();
        if let Some(prefix) = &self.prefix {
            parts.push(format!("{}-", encode(prefix)));
        }
        parts.push(encode(&self.text_start).into_owned());
        if let Some(text_end) = &self.text_end {
            parts.push(encode(text_end).into_owned());
        }
        if let Some(suffix) = &self.suffix {
            parts.push(format!("-{}", encode(suffix)));
        }
        format!("#:~:text={}", parts.join(","))
    }

    /// Parses a Text Fragment from a URL hash string (e.g., "#:~:text=prefix-,start,end,-suffix")
    pub fn from_hash_string(hash: &str) -> Option<Self> {
        // Extract the payload after "#:~:text=" or "~:text="
        let payload = hash.split("~:text=").last().unwrap_or(hash);
        let payload = payload.split('&').next().unwrap_or(payload); // Ignore other hash params

        let parts: Vec<&str> = payload.split(',').collect();
        if parts.is_empty() {
            return None;
        }

        let mut prefix = None;

        let mut idx = 0;

        // 1. Check for prefix (ends with '-')
        if parts[idx].ends_with('-') {
            let p = &parts[idx][..parts[idx].len() - 1];
            prefix = Some(decode(p).ok()?.into_owned());
            idx += 1;
        }

        if idx >= parts.len() {
            return None;
        }

        // 2. Parse text_start
        let text_start = decode(parts[idx]).ok()?.into_owned();
        idx += 1;

        // 3. Check for text_end and suffix
        let mut suffix = None;
        let mut text_end = None;

        if idx < parts.len() {
            if parts[idx].starts_with('-') {
                let s = &parts[idx][1..];
                suffix = Some(decode(s).ok()?.into_owned());
            } else {
                text_end = Some(decode(parts[idx]).ok()?.into_owned());
                idx += 1;
                // If there's still a part left, it must be the suffix
                if idx < parts.len() && parts[idx].starts_with('-') {
                    let s = &parts[idx][1..];
                    suffix = Some(decode(s).ok()?.into_owned());
                }
            }
        }

        Some(TextFragment {
            prefix,
            text_start,
            text_end,
            suffix,
        })
    }
}

pub enum SourceType<'a> {
    PlainText(&'a str),
    HTML(&'a str),
}

pub struct Document<'a> {
    source: &'a str,
    plain_text: String,
    lookup: LineColLookup<'a>,
    source_to_plain_map: Option<Vec<usize>>,
    plain_to_source_map: Option<Vec<usize>>,
}

impl<'a> Document<'a> {
    pub fn source(&'a self) -> SourceType<'a> {
        if self.plain_text.len() == self.source.len() {
            SourceType::PlainText(&self.plain_text)
        } else {
            SourceType::HTML(self.source)
        }
    }

    pub fn plain_text(&'a self) -> &'a str {
        &self.plain_text
    }

    pub fn from_plain_text(text: &'a str) -> Self {
        Self {
            source: text,
            plain_text: text.to_string(),
            lookup: LineColLookup::new(text),
            source_to_plain_map: None,
            plain_to_source_map: None,
        }
    }

    pub fn from_html(html: &'a str) -> Self {
        let (plain_text, s2p, p2s) = Self::strip_html_and_map(html);
        Self {
            source: html,
            plain_text,
            lookup: LineColLookup::new(html),
            source_to_plain_map: Some(s2p),
            plain_to_source_map: Some(p2s),
        }
    }

    fn strip_html_and_map(html: &str) -> (String, Vec<usize>, Vec<usize>) {
        let mut in_tag = false;
        let mut plain_text = String::with_capacity(html.len());
        let mut source_to_plain_map = vec![0; html.len() + 1];
        let mut plain_to_source_map = Vec::with_capacity(html.len());

        for (i, c) in html.char_indices() {
            if c == '<' {
                in_tag = true;
            }

            for byte_offset in 0..c.len_utf8() {
                if i + byte_offset < source_to_plain_map.len() {
                    source_to_plain_map[i + byte_offset] = plain_text.len();
                }
            }

            if !in_tag {
                for byte_offset in 0..c.len_utf8() {
                    plain_to_source_map.push(i + byte_offset);
                }
                plain_text.push(c);
            }

            if c == '>' {
                in_tag = false;
            }
        }

        source_to_plain_map[html.len()] = plain_text.len();
        (plain_text, source_to_plain_map, plain_to_source_map)
    }

    pub fn resolve_to_plain_text_offset(&self, pos: &Position) -> Option<usize> {
        let mut current_line = 1;
        let mut line_start_byte = 0;

        for (i, c) in self.source.char_indices() {
            if current_line == pos.line {
                line_start_byte = i;
                break;
            }
            if c == '\n' {
                current_line += 1;
            }
        }

        let source_byte = line_start_byte
            + self.source[line_start_byte..]
                .chars()
                .take(pos.column - 1)
                .map(|c| c.len_utf8())
                .sum::<usize>();

        if let Some(map) = &self.source_to_plain_map {
            map.get(source_byte).copied()
        } else {
            Some(source_byte)
        }
    }

    /// Converts a plain text start byte offset back into a line-column pair
    pub fn resolve_start_to_source_position(&self, plain_text_byte: usize) -> Option<Position> {
        let source_byte = if let Some(map) = &self.plain_to_source_map {
            *map.get(plain_text_byte)?
        } else {
            plain_text_byte
        };

        let (line, col) = self.lookup.get(source_byte);
        Some(Position { line, column: col })
    }

    /// Converts a plain text end byte offset back into a line-column pair,
    /// explicitly excluding trailing HTML tags from the boundary.
    pub fn resolve_end_to_source_position(&self, plain_text_end_byte: usize) -> Option<Position> {
        if plain_text_end_byte == 0 {
            let (line, col) = self.lookup.get(0);
            return Some(Position { line, column: col });
        }

        // Get the last character of the matched plain text
        let last_char = self.plain_text[..plain_text_end_byte].chars().last()?;
        let last_char_plain_idx = plain_text_end_byte - last_char.len_utf8();

        let source_byte_start = if let Some(map) = &self.plain_to_source_map {
            *map.get(last_char_plain_idx)?
        } else {
            last_char_plain_idx
        };

        // Add the UTF-8 length to place the end boundary exactly after the character,
        // strictly before any subsequent HTML tags (like </span> or </div>)
        let source_byte_end = source_byte_start + last_char.len_utf8();
        let (line, col) = self.lookup.get(source_byte_end);

        Some(Position { line, column: col })
    }
}

pub struct FragmentEngine<'a> {
    doc: Document<'a>,
}

impl<'a> FragmentEngine<'a> {
    pub fn new(doc: Document<'a>) -> Self {
        Self { doc }
    }

    pub fn from_html(html: &'a str) -> Self {
        Self::new(Document::from_html(html))
    }

    pub fn from_plain_text(text: &'a str) -> Self {
        Self::new(Document::from_plain_text(text))
    }

    pub fn plain_text(&'a self) -> &'a str {
        self.doc.plain_text()
    }

    pub fn source(&'a self) -> SourceType<'a> {
        self.doc.source()
    }

    pub fn doc(&'a self) -> &'a Document<'a> {
        &self.doc
    }

    /// Generates a fragment, with an optional robustness configuration to enforce prefix/suffix inclusion.
    pub fn generate(
        &self,
        selection: Selection,
        options: Option<GenerateOptions>,
    ) -> Option<TextFragment> {
        let opts = options.unwrap_or_default();

        let start_byte = self.doc.resolve_to_plain_text_offset(&selection.start)?;
        let end_byte = self.doc.resolve_to_plain_text_offset(&selection.end)?;

        if start_byte >= end_byte || end_byte > self.doc.plain_text.len() {
            return None;
        }

        let selected_text = &self.doc.plain_text[start_byte..end_byte];
        let cleaned_text = self.clean_text(selected_text);
        if cleaned_text.is_empty() {
            return None;
        }

        let (text_start, text_end) = self.determine_start_end(&cleaned_text);

        // Context calculation handles both uniqueness AND the robustness minimums
        let (prefix, suffix) = self.calculate_context(
            start_byte,
            end_byte,
            &text_start,
            text_end.as_deref(),
            &opts,
        );

        Some(TextFragment {
            prefix,
            text_start,
            text_end,
            suffix,
        })
    }

    fn clean_text(&self, text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn determine_start_end(&self, text: &str) -> (String, Option<String>) {
        if text.len() <= MAX_EXACT_MATCH_LENGTH {
            return (text.to_string(), None);
        }
        let words: Vec<&str> = text.split_whitespace().collect();
        let start_words = words
            .iter()
            .take(MIN_CONTEXT_WORDS)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        let end_words = words
            .iter()
            .rev()
            .take(MIN_CONTEXT_WORDS)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        (start_words, Some(end_words))
    }

    fn calculate_context(
        &self,
        start_byte: usize,
        end_byte: usize,
        text_start: &str,
        // TODO: Why is this not used?
        _text_end: Option<&str>,
        opts: &GenerateOptions,
    ) -> (Option<String>, Option<String>) {
        let before_text = &self.doc.plain_text[..start_byte];
        let after_text = &self.doc.plain_text[end_byte..];

        let mut before_words: Vec<&str> = before_text.split_whitespace().rev().collect();
        let mut after_words: Vec<&str> = after_text.split_whitespace().collect();

        let mut prefix_words = Vec::new();
        let mut suffix_words = Vec::new();
        let mut is_unique = false;

        // Loop until unique AND robustness minimums are met
        while (!is_unique
            || prefix_words.len() < opts.min_prefix_words
            || suffix_words.len() < opts.min_suffix_words)
            && (!before_words.is_empty() || !after_words.is_empty())
        {
            if let Some(w) = before_words.first()
                && (prefix_words.len() < opts.min_prefix_words
                    || (!is_unique && prefix_words.len() <= suffix_words.len()))
            {
                prefix_words.insert(0, *w);
                before_words.remove(0);
            }
            if let Some(w) = after_words.first()
                && (suffix_words.len() < opts.min_suffix_words
                    || (!is_unique && suffix_words.len() < prefix_words.len()))
            {
                suffix_words.push(*w);
                after_words.remove(0);
            }

            let p = prefix_words.join(" ");
            let _s = suffix_words.join(" ");

            // Check uniqueness if we aren't already unique
            if !is_unique {
                let pattern = format!("{} {}", p, text_start).trim().to_string();
                is_unique = self
                    .clean_text(&self.doc.plain_text)
                    .matches(&pattern)
                    .count()
                    == 1;
            }
        }

        (
            if prefix_words.is_empty() {
                None
            } else {
                Some(prefix_words.join(" "))
            },
            if suffix_words.is_empty() {
                None
            } else {
                Some(suffix_words.join(" "))
            },
        )
    }

    /// Converts text into a regex pattern that matches any sequence of whitespace (handling newlines/tabs).
    fn build_ws_regex(text: &str) -> String {
        text.split_whitespace()
            .map(regex::escape)
            .collect::<Vec<_>>()
            .join(r"\s+")
    }

    /// Resolves a Text Fragment back into a Line-Column Selection using robust regex whitespace folding.
    pub fn resolve_fragment(&self, fragment: &TextFragment) -> Option<Selection> {
        let mut pattern = String::new();

        // 1. Prefix group (optional match, but ensures context if present)
        if let Some(p) = &fragment.prefix {
            pattern.push_str(&format!("(?P<prefix>{}\\s+)", Self::build_ws_regex(p)));
        }

        // 2. Core group (This captures the actual bounds of the returned text)
        let start_rx = Self::build_ws_regex(&fragment.text_start);
        if let Some(e) = &fragment.text_end {
            let end_rx = Self::build_ws_regex(e);
            // (?s:.*?) allows matching across newlines non-greedily between start and end
            pattern.push_str(&format!("(?P<core>{}(?s:.*?){})", start_rx, end_rx));
        } else {
            pattern.push_str(&format!("(?P<core>{})", start_rx));
        }

        // 3. Suffix group
        if let Some(s) = &fragment.suffix {
            pattern.push_str(&format!("(?P<suffix>\\s+{})", Self::build_ws_regex(s)));
        }

        // Compile and find the first match
        let re = Regex::new(&pattern).ok()?;
        let captures = re.captures(&self.doc.plain_text)?;

        // Extract the exact bounds of the 'core' match (excluding prefix/suffix spaces)
        let core_match = captures.name("core")?;

        let start_pos = self
            .doc
            .resolve_start_to_source_position(core_match.start())?;
        let end_pos = self.doc.resolve_end_to_source_position(core_match.end())?;

        let bytes = ByteIndex::from(core_match);

        Some(Selection {
            start: start_pos,
            end: end_pos,
            bytes,
        })
    }
}
