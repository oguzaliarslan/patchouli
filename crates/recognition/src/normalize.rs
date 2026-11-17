//! Title normalisation, ported from Taiga `track/recognition_normalize.cpp`.
//!
//! Titles from file names and from the database go through the same steps, so
//! that small differences (case, punctuation, `Season 2` vs `2nd Season`,
//! `Ō` vs `ou`) don't prevent a match. The result has no spaces or punctuation.

use caseless::default_case_fold_str;
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

pub fn normalize(title: &str) -> String {
    let mut s: Vec<char> = title.chars().collect();
    normalize_roman_numbers(&mut s);
    transliterate(&mut s);
    let mut s: Vec<char> = normalize_unicode(&s.iter().collect::<String>())
        .chars()
        .collect();
    normalize_ordinal_numbers(&mut s);
    normalize_season_numbers(&mut s);
    for (before, after) in [
        ("&", "and"),
        ("the animation", ""),
        ("the", ""),
        ("episode", ""),
        ("oad", "ova"),
        ("oav", "ova"),
        ("specials", "sp"),
        ("special", "sp"),
        ("(tv)", ""),
    ] {
        replace_whole_word(&mut s, before, after);
    }
    let simplified = s
        .iter()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    erase_punctuation(&simplified)
}

fn is_punctuation(c: char) -> bool {
    use GeneralCategory::*;
    matches!(
        get_general_category(c),
        ConnectorPunctuation
            | DashPunctuation
            | OpenPunctuation
            | ClosePunctuation
            | InitialPunctuation
            | FinalPunctuation
            | OtherPunctuation
    )
}

fn is_boundary(c: char) -> bool {
    c.is_whitespace() || is_punctuation(c)
}

/// Replaces `before` where it is a whole word, i.e. surrounded by the start or
/// end of the text, spaces or punctuation (Taiga's `replaceWholeWord`).
pub fn replace_whole_word(s: &mut Vec<char>, before: &str, after: &str) {
    let before: Vec<char> = before.chars().collect();
    let after: Vec<char> = after.chars().collect();
    if before.is_empty() {
        return;
    }
    let find = |s: &[char], from: usize| {
        (from..=s.len().saturating_sub(before.len()))
            .find(|&i| i + before.len() <= s.len() && s[i..i + before.len()] == before[..])
    };
    let mut pos = find(s, 0);
    while let Some(p) = pos {
        let end = p + before.len();
        let whole = (p == 0 || is_boundary(s[p - 1])) && (end >= s.len() || is_boundary(s[end]));
        if whole {
            s.splice(p..end, after.iter().copied());
            pos = find(s, p + after.len());
        } else {
            pos = find(s, end);
        }
    }
}

fn normalize_roman_numbers(s: &mut Vec<char>) {
    // 1 and 10 are skipped, as "I" and "X" are rarely numerals in titles.
    for (before, after) in [
        ("II", "2"),
        ("III", "3"),
        ("IV", "4"),
        ("V", "5"),
        ("VI", "6"),
        ("VII", "7"),
        ("VIII", "8"),
        ("IX", "9"),
        ("XI", "11"),
        ("XII", "12"),
        ("XIII", "13"),
    ] {
        replace_whole_word(s, before, after);
    }
}

fn normalize_ordinal_numbers(s: &mut Vec<char>) {
    for (before, after) in [
        ("first", "1st"),
        ("second", "2nd"),
        ("third", "3rd"),
        ("fourth", "4th"),
        ("fifth", "5th"),
        ("sixth", "6th"),
        ("seventh", "7th"),
        ("eighth", "8th"),
        ("ninth", "9th"),
    ] {
        replace_whole_word(s, before, after);
    }
}

fn normalize_season_numbers(s: &mut Vec<char>) {
    for (after, list) in [
        ("1", ["1st season", "season 1", "series 1", "s1"]),
        ("2", ["2nd season", "season 2", "series 2", "s2"]),
        ("3", ["3rd season", "season 3", "series 3", "s3"]),
        ("4", ["4th season", "season 4", "series 4", "s4"]),
        ("5", ["5th season", "season 5", "series 5", "s5"]),
        ("6", ["6th season", "season 6", "series 6", "s6"]),
    ] {
        for before in list {
            replace_whole_word(s, before, after);
        }
    }
}

fn transliterate(s: &mut Vec<char>) {
    let mut out = Vec::with_capacity(s.len());
    for &c in s.iter() {
        match c {
            '@' => out.push('a'),                // "iDOLM@STER"
            '×' => out.push('x'),                // "Tasogare Otome × Amnesia"
            '\u{A789}' => out.push(':'),         // modifier letter colon
            'Ō' | 'ō' => out.extend(['o', 'u']), // common romanizations
            'ū' => out.extend(['u', 'u']),
            _ => out.push(c),
        }
    }
    *s = out;
    // Hepburn to Wapuro romanization
    replace_whole_word(s, "wa", "ha");
    replace_whole_word(s, "e", "he");
    replace_whole_word(s, "o", "wo");
}

/// utf8proc's `IGNORE`: default-ignorable code points.
fn is_default_ignorable(c: char) -> bool {
    matches!(c as u32,
        0x00AD | 0x034F | 0x061C | 0x115F..=0x1160 | 0x17B4..=0x17B5 | 0x180B..=0x180F
        | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F | 0x3164 | 0xFE00..=0xFE0F
        | 0xFEFF | 0xFFA0 | 0xFFF0..=0xFFF8 | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A
        | 0xE0000..=0xE0FFF)
}

/// utf8proc's `LUMP`: maps look-alike characters to ASCII.
fn lump(c: char) -> char {
    match c {
        c if c != ' ' && get_general_category(c) == GeneralCategory::SpaceSeparator => ' ',
        '\u{2018}' | '\u{2019}' | '\u{02BC}' | '\u{02C8}' | '\u{2032}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{02BA}' | '\u{02DD}' | '\u{2033}' => '"',
        '\u{2010}'..='\u{2015}' | '\u{2043}' | '\u{2212}' => '-',
        '\u{2044}' | '\u{2215}' => '/',
        '\u{2236}' => ':',
        '\u{2039}' | '\u{2329}' | '\u{3008}' => '<',
        '\u{203A}' | '\u{232A}' | '\u{3009}' => '>',
        '\u{2216}' => '\\',
        '\u{02C4}' | '\u{02C6}' | '\u{2038}' | '\u{2303}' => '^',
        '\u{02CD}' | '\u{2017}' => '_',
        '\u{02CB}' | '\u{2035}' => '`',
        '\u{2223}' => '|',
        '\u{223C}' => '~',
        '\u{2028}' | '\u{2029}' => '\n',
        _ => c,
    }
}

/// Taiga runs utf8proc with COMPAT, COMPOSE, STABLE, IGNORE, STRIPCC,
/// STRIPMARK, LUMP and CASEFOLD: NFKC, minus ignorable and control
/// characters and marks (accents, dakuten), with look-alikes mapped to ASCII,
/// case-folded.
fn normalize_unicode(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|&c| !is_default_ignorable(c))
        .filter_map(|c| {
            let code = c as u32;
            if code < 0x20 || (0x7F..0xA0).contains(&code) {
                (c == '\t').then_some(' ')
            } else {
                Some(lump(c))
            }
        })
        .collect();
    let folded = default_case_fold_str(&cleaned);
    folded
        .nfkd()
        .filter(|&c| !is_combining_mark(c) && !is_default_ignorable(c))
        .map(lump)
        .nfc()
        .collect()
}

/// Removes spaces, ASCII/Latin-1 punctuation and symbols, and the stars,
/// hearts and notes of U+2001–U+2766.
fn erase_punctuation(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            let code = c as u32;
            let latin1_symbol = code <= 0xFF && !c.is_alphanumeric();
            let decoration = code > 0x2000 && code < 0x2767;
            !latin1_symbol && !decoration
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_case_spaces_and_punctuation() {
        assert_eq!(normalize("Toradora!"), "toradora");
        assert_eq!(normalize("Re:Zero kara Hajimeru"), "rezerokarahajimeru");
        assert_eq!(normalize("Love Live! Sunshine!!"), "lovelivesunshine");
        assert_eq!(normalize("Hataraku Maou-sama! ★"), "hatarakumaousama");
    }

    #[test]
    fn unicode_folding() {
        assert_eq!(normalize("Café"), "cafe");
        assert_eq!(normalize("ＡＢＣ"), "abc");
        assert_eq!(normalize("Ōkami"), "oukami");
        assert_eq!(normalize("Straße"), "strasse");
    }

    #[test]
    fn season_and_ordinal_forms_agree() {
        let expected = "shingekinokyojin2";
        assert_eq!(normalize("Shingeki no Kyojin Season 2"), expected);
        assert_eq!(normalize("Shingeki no Kyojin 2nd Season"), expected);
        assert_eq!(normalize("Shingeki no Kyojin Second Season"), expected);
        assert_eq!(normalize("Shingeki no Kyojin S2"), expected);
        assert_eq!(normalize("Shingeki no Kyojin II"), expected);
    }

    #[test]
    fn romanization_and_common_words() {
        assert_eq!(
            normalize("Kaguya-sama wa Kokurasetai"),
            normalize("Kaguya-sama ha Kokurasetai")
        );
        assert_eq!(normalize("The iDOLM@STER"), "idolmaster");
        assert_eq!(normalize("Hellsing OAV"), normalize("Hellsing OVA"));
        assert_eq!(normalize("Title (TV)"), "title");
        assert_eq!(normalize("Kimi & Boku"), "kimiandboku");
    }

    #[test]
    fn roman_numerals_need_word_boundaries() {
        assert_eq!(normalize("Fate/Zero II"), "fatezero2");
        // `V` inside a word stays.
        assert_eq!(normalize("Vivy"), "vivy");
    }

    #[test]
    fn whole_word_replacement() {
        let mut s: Vec<char> = "the theme of the".chars().collect();
        replace_whole_word(&mut s, "the", "a");
        assert_eq!(s.iter().collect::<String>(), "a theme of a");
    }
}
