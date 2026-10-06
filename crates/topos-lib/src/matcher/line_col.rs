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
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        // For ASCII, every kind of column is the byte column
        let (chars, utf16) = if before.is_ascii() {
            (before.len(), before.len())
        } else {
            (before.chars().count(), before.encode_utf16().count())
        };
        Position {
            line: line + 1,
            column: before.len() + 1,
            char_column: chars + 1,
            utf16_column: utf16 + 1,
        }
    }
}

impl LineIndex<'_> {
    /**
    The byte offset of a 0-based line and UTF-16 column (an LSP position)
    - A column past the end of the line clamps to the end, as LSP specifies
    - [`None`] for a line past the end, or a column inside a character
    */
    pub fn offset_of_utf16(&self, line: usize, utf16_column: usize) -> Option<usize> {
        let start = *self.line_starts.get(line)?;
        let end = self
            .line_starts
            .get(line + 1)
            .map_or(self.text.len(), |next| next - 1);
        let mut units = 0;
        for (idx, c) in self.text[start..end].char_indices() {
            if units >= utf16_column {
                return (units == utf16_column).then_some(start + idx);
            }
            units += c.len_utf16();
        }
        Some(end)
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

    #[test]
    fn offsets_from_lsp_positions() {
        let text = "ab\nJé 日𝄞x\n";
        let index = LineIndex::new(text);
        assert_eq!(index.offset_of_utf16(0, 0), Some(0));
        assert_eq!(index.offset_of_utf16(1, 2), Some(3 + "Jé".len()));
        // `𝄞` is 2 UTF-16 units, so column 6 is after it and column 5 is inside it
        assert_eq!(index.offset_of_utf16(1, 6), Some(3 + "Jé 日𝄞".len()));
        assert_eq!(index.offset_of_utf16(1, 5), None);
        // past the end of a line clamps; past the last line is an error
        assert_eq!(index.offset_of_utf16(0, 99), Some(2));
        assert_eq!(index.offset_of_utf16(9, 0), None);
    }
}
