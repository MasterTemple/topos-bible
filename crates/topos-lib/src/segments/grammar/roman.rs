/// Values of the Roman numerals that fit in a [`u8`], largest first (subtractive pairs included)
const NUMERALS: [(&str, u8); 9] = [
    ("C", 100),
    ("XC", 90),
    ("L", 50),
    ("XL", 40),
    ("X", 10),
    ("IX", 9),
    ("V", 5),
    ("IV", 4),
    ("I", 1),
];

pub fn is_numeral(b: u8) -> bool {
    matches!(b.to_ascii_uppercase(), b'I' | b'V' | b'X' | b'L' | b'C')
}

/**
- Parses a case-insensitive Roman numeral up to 255
- Only the canonical form is accepted (`IV`, not `IIII`), so ordinary words made of numeral
letters (like `civil`) are rejected
*/
pub fn parse(s: &str) -> Option<u8> {
    let upper = s.to_ascii_uppercase();
    let mut rest = upper.as_str();
    let mut value: u8 = 0;
    for (numeral, amount) in NUMERALS {
        while let Some(next) = rest.strip_prefix(numeral) {
            value = value.checked_add(amount)?;
            rest = next;
        }
    }
    (rest.is_empty() && value > 0 && to_roman(value) == upper).then_some(value)
}

fn to_roman(mut value: u8) -> String {
    let mut out = String::new();
    for (numeral, amount) in NUMERALS {
        while value >= amount {
            out.push_str(numeral);
            value -= amount;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_value() {
        for value in 1..=255u8 {
            let numeral = to_roman(value);
            assert_eq!(parse(&numeral), Some(value), "{numeral}");
            assert_eq!(parse(&numeral.to_lowercase()), Some(value), "{numeral}");
        }
    }

    #[test]
    fn rejects_non_canonical() {
        for s in ["", "IIII", "VX", "IC", "CCLVI", "civil", "lil"] {
            assert_eq!(parse(s), None, "{s}");
        }
    }
}
