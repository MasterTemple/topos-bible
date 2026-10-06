use crate::matcher::{
    bible_matcher::{BibleMatcher, MatchResult, Matcher},
    instance::{BibleMatch, FoundPassage},
    text::SearchText,
};

#[derive(Copy, Clone, Debug)]
pub struct ByteIndex {
    pub start: usize,
    pub end: usize,
}

impl ByteIndex {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// A position in text; every column is 1-based (subtract 1 for LSP, which is 0-based)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Position {
    /// 1-based line
    pub line: usize,
    /// 1-based column in bytes (what Vim and Neovim use)
    pub column: usize,
    /// 1-based column in characters
    pub char_column: usize,
    /// 1-based column in UTF-16 code units (what LSP and JavaScript use)
    pub utf16_column: usize,
}

/// Converts byte offsets into [`Position`]s, by binary searching the start of each line
#[derive(Clone, Debug)]
pub struct LineIndex<'a> {
    text: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(idx, _)| idx + 1))
            .collect();
        Self { text, line_starts }
    }

    /// The position of a byte offset (which must be on a char boundary)
    pub fn position(&self, offset: usize) -> Position {
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let before = &self.text[self.line_starts[line]..offset];
        Position {
            line: line + 1,
            column: before.len() + 1,
            char_column: before.chars().count() + 1,
            utf16_column: before.encode_utf16().count() + 1,
        }
    }
}

/// Where a match is in plain text: byte offsets, plus line and column positions
#[derive(Copy, Clone, Debug)]
pub struct LineColLocation {
    pub start: Position,
    pub end: Position,
    pub bytes: ByteIndex,
}

impl LineColLocation {
    pub fn new(index: &LineIndex, start: usize, end: usize) -> Self {
        Self {
            start: index.position(start),
            end: index.position(end),
            bytes: ByteIndex::new(start, end),
        }
    }
}

impl Matcher for LineColLocation {
    type Input<'a> = &'a str;

    /// - This always returns the [`Ok`] variant
    /// - Using the [`Result::unwrap_or_default()`] method results in an empty [`Vec`], so just do that
    fn search<'a>(
        matcher: &BibleMatcher,
        input: Self::Input<'a>,
    ) -> MatchResult<Vec<BibleMatch<Self>>> {
        let mut filtered = matcher.filter();
        let text = SearchText::new(input);
        let index = LineIndex::new(input);
        let data = matcher.data();

        let starts: Vec<_> = data.books().candidates(text.as_str()).collect();
        let mut found: Vec<FoundPassage> = starts
            .iter()
            .enumerate()
            .filter_map(|(idx, cur)| {
                let next_start = starts.get(idx + 1).map(|next| next.start());
                FoundPassage::find(data, text.as_str(), *cur, next_start)
            })
            .collect();

        // References without a book name, when the document's book is known
        if let Some(context) = matcher.context() {
            let taken: Vec<_> = found.iter().map(|f| f.bytes.clone()).collect();
            let book_starts: Vec<_> = starts.iter().map(|s| s.start()).collect();
            found.extend(context.find_bare(data, text.as_str(), &taken, &book_starts));
            found.sort_by_key(|f| f.bytes.start);
        }

        for found in found {
            let bytes = text.original_range(found.bytes);
            let location = LineColLocation::new(&index, bytes.start, bytes.end);
            filtered.try_add(BibleMatch {
                location,
                psg: found.psg,
            });
        }

        let matches = filtered.matches();
        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions() {
        let index = LineIndex::new("ab\nJé 日𝄞x\n");
        let at = |offset| {
            let p = index.position(offset);
            (p.line, p.column, p.char_column, p.utf16_column)
        };
        assert_eq!(at(0), (1, 1, 1, 1));
        assert_eq!(at(2), (1, 3, 3, 3));
        assert_eq!(at(3), (2, 1, 1, 1));
        // `é` is 2 bytes, `日` is 3 bytes, and `𝄞` is 4 bytes and 2 UTF-16 units
        let x = "ab\nJé 日𝄞".len();
        assert_eq!(at(x), (2, 12, 6, 7));
        assert_eq!(at(x + 2), (3, 1, 1, 1));
    }
}
