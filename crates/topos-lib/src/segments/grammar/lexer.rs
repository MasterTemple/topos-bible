use crate::segments::grammar::{
    roman,
    tree::{Delimiter, DelimiterKind, Following, Number, NumberKind, Span},
};

/// Characters that can follow a verse number to mark part of a verse (`Matthew 28:18b`)
const SUBVERSE: &str = "abcd";

/// Spaces allowed between a number and its unit (`5 %`), including non-breaking ones
pub const SPACES: [char; 3] = [' ', '\u{a0}', '\u{202f}'];

/// Symbols that make a number a quantity rather than a chapter or verse (`5%`, `1.5°`)
pub fn is_unit_symbol(c: char) -> bool {
    matches!(
        c,
        '%' | '‰' | '‱' | '°' | '℃' | '℉' | '′' | '″' | '€' | '£' | '¥' | '¢'
    )
}

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
        // A blank line (like between paragraphs or subtitle cues) ends a reference
        if self.input[self.pos..start].matches('\n').count() >= 2 {
            return None;
        }
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

    // A subverse letter must end the word (`16a`)
    let after = &s[digits..];
    let subverse = after
        .chars()
        .next()
        .filter(|c| SUBVERSE.contains(*c) && at_boundary(&after[1..]));
    let mut len = digits + subverse.map_or(0, char::len_utf8);

    // `16f` (and the next verse) or `16ff` (and the rest of the chapter), maybe after a space
    let mut following = None;
    if subverse.is_none() {
        let after = &s[len..];
        let spaces = after.len() - after.trim_start_matches(' ').len();
        let suffix = &after[spaces..];
        for (word, kind) in [("ff", Following::Rest), ("f", Following::Next)] {
            if suffix.starts_with(word) && at_boundary(&suffix[word.len()..]) {
                following = Some(kind);
                len += spaces + word.len();
                break;
            }
        }
    }

    // Otherwise a number must end the word, so `GA9hNK` is not Galatians 9
    if !at_boundary(&s[len..]) {
        return None;
    }
    // A number with a unit symbol is a quantity (`5%`, `5 %`)
    if s[len..]
        .trim_start_matches(SPACES)
        .chars()
        .next()
        .is_some_and(is_unit_symbol)
    {
        return None;
    }

    let number = Number {
        value,
        kind: NumberKind::Decimal,
        subverse,
        following,
        span: Span::new(start, start + len),
    };
    Some((number, len))
}

fn lex_roman(s: &str, start: usize) -> Option<(Number, usize)> {
    let len = s.bytes().take_while(|b| roman::is_numeral(*b)).count();
    if len == 0 || len > MAX_ROMAN || !at_boundary(&s[len..]) {
        return None;
    }
    // One case throughout (`iv` or `IV`, not `vI`), as in real references
    let numeral = &s[..len];
    if numeral != numeral.to_ascii_lowercase() && numeral != numeral.to_ascii_uppercase() {
        return None;
    }
    let value = roman::parse(numeral)?;

    let number = Number {
        value,
        kind: NumberKind::Roman,
        subverse: None,
        following: None,
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
        // numbers must end the word
        assert_eq!(kinds("16and"), Vec::<String>::new());
        assert_eq!(kinds("9hNKdqbH"), Vec::<String>::new());
        assert_eq!(kinds("16a, 17"), ["16", ",", "17"]);
        // a blank line ends the reference
        assert_eq!(kinds("6, 24.\n\n143"), ["6", ",", "24", "."]);
        assert_eq!(kinds("6,\n24"), ["6", ",", "24"]);
        // all Roman numeral letters, but not a canonical numeral
        assert_eq!(kinds("8, civil"), ["8", ","]);
        // Roman numerals must be a whole word in one case
        assert_eq!(kinds("8, xylophone"), ["8", ","]);
        assert_eq!(kinds("vI"), Vec::<String>::new());
        assert_eq!(kinds("VI, vi"), ["6", ",", "6"]);
        // non-ASCII digits are not numbers
        assert_eq!(kinds("٣"), Vec::<String>::new());
    }

    #[test]
    fn following_verses() {
        let following = |input: &str| match Lexer::new(input).next() {
            Some(Token::Number(n)) => (n.following, n.span.end),
            _ => panic!("{input}"),
        };
        assert_eq!(following("16f"), (Some(Following::Next), 3));
        assert_eq!(following("16ff"), (Some(Following::Rest), 4));
        assert_eq!(following("16 ff."), (Some(Following::Rest), 5));
        assert_eq!(following("16 for"), (None, 2));
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
