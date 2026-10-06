use crate::segments::grammar::{
    roman,
    tree::{Delimiter, DelimiterKind, Number, NumberKind, Span},
};

/// Characters that can follow a verse number to mark part of a verse (`Matthew 28:18b`)
const SUBVERSE: &str = "abcd";

/// - Decimal numbers are at most 3 digits
/// - Roman numerals are at most 9 characters, just to keep the lexer from getting trolled
const MAX_DIGITS: usize = 3;
const MAX_ROMAN: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Number(Number),
    Delimiter(Delimiter),
}

/**
- Lazily splits segment input into [`Token`]s, skipping whitespace
- It stops at the first character that cannot start a token, because the segment window can be
  the rest of an entire document
- Whitespace is not emitted: it is always the gap between two token spans, so the tree stays lossless
*/
pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        let rest = &self.input[self.pos..];
        let trimmed = rest.trim_start();
        let start = self.pos + (rest.len() - trimmed.len());
        let c = trimmed.chars().next()?;

        let (token, len) = if let Some(kind) = DelimiterKind::from_char(c) {
            let len = c.len_utf8();
            let span = Span::new(start, start + len);
            let delimiter = Delimiter {
                kind,
                actual: c,
                span,
            };
            (Token::Delimiter(delimiter), len)
        } else if c.is_ascii_digit() {
            let (number, len) = lex_decimal(trimmed, start)?;
            (Token::Number(number), len)
        } else {
            let (number, len) = lex_roman(trimmed, start)?;
            (Token::Number(number), len)
        };

        self.pos = start + len;
        Some(token)
    }
}

/// `true` when the text starts at a word boundary (the end of input counts)
fn at_boundary(s: &str) -> bool {
    s.chars().next().is_none_or(|c| !c.is_alphanumeric())
}

fn lex_decimal(s: &str, start: usize) -> Option<(Number, usize)> {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    if digits > MAX_DIGITS {
        return None;
    }
    let value: u8 = s[..digits].parse().ok()?;

    // Only take a subverse letter when it is not the start of a word (`16a` but not `16and`)
    let after = &s[digits..];
    let subverse = after
        .chars()
        .next()
        .filter(|c| SUBVERSE.contains(*c) && at_boundary(&after[1..]));
    let len = digits + subverse.map_or(0, char::len_utf8);

    let number = Number {
        value,
        kind: NumberKind::Decimal,
        subverse,
        span: Span::new(start, start + len),
    };
    Some((number, len))
}

fn lex_roman(s: &str, start: usize) -> Option<(Number, usize)> {
    let len = s.bytes().take_while(|b| roman::is_numeral(*b)).count();
    if len == 0 || len > MAX_ROMAN || !at_boundary(&s[len..]) {
        return None;
    }
    let value = roman::parse(&s[..len])?;

    let number = Number {
        value,
        kind: NumberKind::Roman,
        subverse: None,
        span: Span::new(start, start + len),
    };
    Some((number, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<String> {
        Lexer::new(input)
            .map(|t| match t {
                Token::Number(n) => n.value.to_string(),
                Token::Delimiter(d) => d.actual.to_string(),
            })
            .collect()
    }

    #[test]
    fn lexes_tokens() {
        assert_eq!(kinds(" 1 : 2 – 3"), ["1", ":", "2", "–", "3"]);
        assert_eq!(kinds("16a-18"), ["16", "-", "18"]);
        assert_eq!(kinds("x, 8"), ["10", ",", "8"]);
    }

    #[test]
    fn stops_at_invalid_input() {
        // more than 3 digits
        assert_eq!(kinds("3, 2025"), ["3", ","]);
        // not a word boundary after the subverse
        assert_eq!(kinds("16and"), ["16"]);
        // all Roman numeral letters, but not a canonical numeral
        assert_eq!(kinds("8, civil"), ["8", ","]);
        // Roman numerals must be a whole word
        assert_eq!(kinds("8, xylophone"), ["8", ","]);
        // non-ASCII digits are not numbers
        assert_eq!(kinds("٣"), Vec::<String>::new());
    }

    #[test]
    fn spans_are_byte_offsets() {
        let tokens: Vec<_> = Lexer::new("1 — 2").collect();
        let Token::Number(end) = tokens[2] else {
            panic!()
        };
        // `—` is 3 bytes
        assert_eq!(end.span, Span::new(6, 7));
    }
}
