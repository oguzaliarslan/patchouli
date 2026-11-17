//! Character classes (`detail/delimiter.hpp`, `detail/bracket.hpp`).

pub fn is_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\u{00A0}' | '\u{200B}' | '\u{3000}')
}

pub fn is_dash(ch: char) -> bool {
    matches!(
        ch,
        '-' | '\u{00AD}'
            | '\u{2010}'
            | '\u{2011}'
            | '\u{2012}'
            | '\u{2013}'
            | '\u{2014}'
            | '\u{2015}'
    )
}

pub fn is_delimiter(ch: char) -> bool {
    matches!(ch, '_' | '.' | ',' | '&' | '~' | '+' | '|' | ':') || is_space(ch) || is_dash(ch)
}

pub fn is_open_bracket(ch: char) -> bool {
    matches!(
        ch,
        '(' | '['
            | '{'
            | '\u{300C}'
            | '\u{300E}'
            | '\u{3010}'
            | '\u{FF08}'
            | '\u{FF3B}'
            | '\u{FF5B}'
    )
}

pub fn is_close_bracket(ch: char) -> bool {
    matches!(
        ch,
        ')' | ']'
            | '}'
            | '\u{300D}'
            | '\u{300F}'
            | '\u{3011}'
            | '\u{FF09}'
            | '\u{FF3D}'
            | '\u{FF5D}'
    )
}

pub fn is_bracket(ch: char) -> bool {
    is_open_bracket(ch) || is_close_bracket(ch)
}

/// Text is anything that isn't a bracket or a delimiter.
pub fn is_text(ch: char) -> bool {
    !is_bracket(ch) && !is_delimiter(ch)
}

/// The first *byte* of a string as a character. The C++ code tests
/// `value.front()`, a `char`, in a few places, so multi-byte dashes and spaces
/// don't count there; this keeps that behaviour.
pub fn first_byte(value: &str) -> char {
    value.as_bytes().first().map_or('\0', |&b| char::from(b))
}

/// `"1st"` / `"First"` → `"1"`, up to 9 (case-sensitive, as in C++).
pub fn from_ordinal_number(input: &str) -> Option<&'static str> {
    Some(match input {
        "1st" | "First" => "1",
        "2nd" | "Second" => "2",
        "3rd" | "Third" => "3",
        "4th" | "Fourth" => "4",
        "5th" | "Fifth" => "5",
        "6th" | "Sixth" => "6",
        "7th" | "Seventh" => "7",
        "8th" | "Eighth" => "8",
        "9th" | "Ninth" => "9",
        _ => return None,
    })
}

pub fn from_roman_number(input: &str) -> Option<&'static str> {
    Some(match input {
        "II" => "2",
        "III" => "3",
        "IV" => "4",
        _ => return None,
    })
}

/// `std::from_chars` into an `int`: 0 on failure or overflow.
pub fn to_int(value: &str) -> i64 {
    let digits: &str = value
        .find(|c: char| !c.is_ascii_digit())
        .map_or(value, |end| &value[..end]);
    digits.parse::<i32>().map(i64::from).unwrap_or(0)
}
