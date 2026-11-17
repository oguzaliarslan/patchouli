//! Element parsers (`detail/parser.hpp`, `detail/parser/*.hpp`).
//!
//! Each parser marks the tokens it uses (`Token::element`), so later parsers
//! only see free tokens. Ranges are `start..end` indices into the token list.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::chars::{first_byte, from_ordinal_number, from_roman_number, is_space, to_int};
use crate::keyword::KeywordKind;
use crate::token::{Token, TokenKind};
use crate::{Element, ElementKind, Options, value};

pub fn parse(tokens: &mut [Token], options: &Options) -> Vec<Element> {
    let mut elements = Vec::new();
    if options.parse_file_extension {
        elements.extend(parse_file_extension(tokens));
    }
    elements.extend(parse_keywords(tokens, options));
    if options.parse_file_checksum {
        elements.extend(parse_file_checksum(tokens));
    }
    if options.parse_video_resolution {
        elements.extend(parse_video_resolution(tokens));
    }
    if options.parse_year {
        elements.extend(parse_year(tokens));
    }
    if options.parse_season {
        elements.extend(parse_season(tokens));
    }
    if options.parse_part {
        elements.extend(parse_part(tokens));
    }
    if options.parse_episode {
        elements.extend(parse_volume(tokens));
        elements.extend(parse_episode(tokens));
    }
    if options.parse_title {
        elements.extend(parse_title(tokens));
    }
    let contains = |elements: &[Element], kind| elements.iter().any(|e: &Element| e.kind == kind);
    if options.parse_release_group && !contains(&elements, ElementKind::ReleaseGroup) {
        elements.extend(parse_release_group(tokens));
    }
    if options.parse_episode_title && contains(&elements, ElementKind::Episode) {
        elements.extend(parse_episode_title(tokens));
    }
    elements.sort_by_key(|e| e.position);
    elements
}

// --- Helpers -------------------------------------------------------------

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid pattern")
}

/// Element from a token, optionally with another value or position.
fn element_from(
    kind: ElementKind,
    token: &Token,
    value: Option<&str>,
    position: Option<usize>,
) -> Element {
    Element::new(
        kind,
        value.filter(|v| !v.is_empty()).unwrap_or(&token.value),
        position.unwrap_or(token.position),
    )
}

/// Index of the last token before `end` matching `predicate`.
fn find_prev(tokens: &[Token], end: usize, predicate: impl Fn(&Token) -> bool) -> Option<usize> {
    tokens[..end.min(tokens.len())].iter().rposition(predicate)
}

/// Index of the first token after `index` matching `predicate`.
fn find_next(tokens: &[Token], index: usize, predicate: impl Fn(&Token) -> bool) -> Option<usize> {
    if index >= tokens.len() {
        return None;
    }
    tokens[index + 1..]
        .iter()
        .position(predicate)
        .map(|i| i + index + 1)
}

/// Index of the first token at or after `start` matching `predicate`, or `len`.
fn find_from(tokens: &[Token], start: usize, predicate: impl Fn(&Token) -> bool) -> usize {
    tokens
        .get(start..)
        .and_then(|rest| rest.iter().position(predicate))
        .map_or(tokens.len(), |i| i + start)
}

fn is_not_delimiter(token: &Token) -> bool {
    !token.is_delimiter()
}

fn mark(tokens: &mut [Token], range: std::ops::Range<usize>, kind: ElementKind) {
    for token in &mut tokens[range] {
        token.element = Some(kind);
    }
}

/// Byte offset of capture group `i`.
fn group_start(captures: &Captures, i: usize) -> usize {
    captures.get(i).map_or(0, |m| m.start())
}

fn group<'a>(captures: &Captures<'a>, i: usize) -> Option<&'a str> {
    captures.get(i).map(|m| m.as_str())
}

// --- Keywords ------------------------------------------------------------

fn keyword_element_kind(kind: KeywordKind) -> ElementKind {
    use KeywordKind::*;
    match kind {
        AudioChannels | AudioCodec | AudioLanguage => ElementKind::AudioTerm,
        Device => ElementKind::Device,
        Episode => ElementKind::Episode,
        EpisodeType | Type => ElementKind::Type,
        Language => ElementKind::Language,
        Other => ElementKind::Other,
        Part => ElementKind::Part,
        ReleaseGroup => ElementKind::ReleaseGroup,
        ReleaseInformation => ElementKind::ReleaseInformation,
        ReleaseVersion => ElementKind::ReleaseVersion,
        Season => ElementKind::Season,
        Source => ElementKind::Source,
        Subtitles | SubtitleLanguage => ElementKind::Subtitles,
        VideoCodec | VideoColorDepth | VideoDynamicRange | VideoFormat | VideoFrameRate
        | VideoProfile | VideoQuality => ElementKind::VideoTerm,
        VideoResolution => ElementKind::VideoResolution,
        Volume => ElementKind::Volume,
    }
}

fn parse_keywords(tokens: &mut [Token], options: &Options) -> Vec<Element> {
    let mut elements = Vec::new();
    for token in tokens.iter_mut().filter(|t| t.is_keyword()) {
        let Some(keyword) = token.keyword else {
            continue;
        };
        let allowed = match keyword.kind {
            KeywordKind::ReleaseGroup => options.parse_release_group,
            KeywordKind::VideoResolution => options.parse_video_resolution,
            _ => true,
        };
        if !allowed {
            continue;
        }
        let kind = keyword_element_kind(keyword.kind);
        if !keyword.is_ambiguous() || token.enclosed {
            token.element = Some(kind);
        }
        // Prefixes (`Episode`, `Part`, `Season`, `Vol`) become elements with
        // the number that follows them, in other parsers.
        let is_prefix = matches!(
            keyword.kind,
            KeywordKind::Episode | KeywordKind::Part | KeywordKind::Season | KeywordKind::Volume
        );
        if !is_prefix {
            let value = if keyword.kind == KeywordKind::ReleaseVersion {
                token.value.get(1..).unwrap_or_default() // `v2` -> `2`
            } else {
                &token.value
            };
            elements.push(Element::new(kind, value, token.position));
        }
    }
    elements
}

// --- File extension, checksum, resolution, year --------------------------

const FILE_EXTENSIONS: [&str; 16] = [
    "3gp", "avi", "divx", "flv", "m2ts", "m4v", "mkv", "mov", "mp4", "mpg", "ogm", "rm", "rmvb",
    "ts", "webm", "wmv",
];

fn parse_file_extension(tokens: &mut [Token]) -> Option<Element> {
    let n = tokens.len();
    if n < 2 {
        return None;
    }
    let (prev, last) = (&tokens[n - 2], &tokens[n - 1]);
    let is_extension =
        (last.is_keyword() || last.is_text()) && FILE_EXTENSIONS.contains(&last.value.as_str());
    let is_dot = prev.is_delimiter() && prev.value == ".";
    if !is_extension || !is_dot {
        return None;
    }
    let last = &mut tokens[n - 1];
    last.kind = TokenKind::Text; // in case it was a keyword (e.g. `TS`)
    last.keyword = None;
    last.element = Some(ElementKind::FileExtension);
    Some(element_from(ElementKind::FileExtension, last, None, None))
}

fn parse_file_checksum(tokens: &mut [Token]) -> Option<Element> {
    // A CRC-32 checksum has 8 hexadecimal digits; take the last one.
    let token = tokens.iter_mut().rev().find(|t| {
        t.is_free() && t.value.len() == 8 && t.value.bytes().all(|b| b.is_ascii_hexdigit())
    })?;
    token.element = Some(ElementKind::FileChecksum);
    Some(element_from(ElementKind::FileChecksum, token, None, None))
}

fn parse_video_resolution(tokens: &mut [Token]) -> Vec<Element> {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| regex(r"^[0-9]{3,4}(?:[ipP]|(?:x|X|×)[0-9]{3,4}[ipP]?)$"));
    let mut elements = Vec::new();
    for token in tokens
        .iter_mut()
        .filter(|t| t.is_free() && PATTERN.is_match(&t.value))
    {
        token.element = Some(ElementKind::VideoResolution);
        elements.push(element_from(
            ElementKind::VideoResolution,
            token,
            None,
            None,
        ));
    }
    if elements.is_empty()
        && let Some(token) = tokens
            .iter_mut()
            .find(|t| t.is_free() && t.number && t.value == "1080")
    {
        token.element = Some(ElementKind::VideoResolution);
        elements.push(element_from(
            ElementKind::VideoResolution,
            token,
            None,
            None,
        ));
    }
    elements
}

/// `[number]`: a free number between an opening and a closing bracket.
fn is_isolated_number(tokens: &[Token], i: usize) -> bool {
    i >= 1
        && i + 1 < tokens.len()
        && tokens[i - 1].kind == TokenKind::OpenBracket
        && tokens[i + 1].kind == TokenKind::CloseBracket
        && tokens[i].is_free()
        && tokens[i].number
}

fn parse_year(tokens: &mut [Token]) -> Option<Element> {
    let i = (1..tokens.len().saturating_sub(1)).find(|&i| {
        let year = to_int(&tokens[i].value);
        is_isolated_number(tokens, i) && 1950 < year && year < 2050
    })?;
    tokens[i].element = Some(ElementKind::Year);
    Some(element_from(ElementKind::Year, &tokens[i], None, None))
}

// --- Season, part, volume --------------------------------------------------

fn parse_season(tokens: &mut [Token]) -> Vec<Element> {
    let mut elements = Vec::new();
    let is_season_keyword = |t: &Token| t.keyword_kind() == Some(KeywordKind::Season);

    // `2nd Season`, `Season 2`, `Season II`
    for i in 0..tokens.len().saturating_sub(2) {
        let (a, b, c) = (i, i + 1, i + 2);
        if is_season_keyword(&tokens[c]) && tokens[b].is_delimiter() && tokens[a].is_free() {
            if let Some(number) = from_ordinal_number(&tokens[a].value) {
                tokens[a].element = Some(ElementKind::Season);
                tokens[c].element = Some(ElementKind::Season);
                elements.push(Element::new(
                    ElementKind::Season,
                    number,
                    tokens[a].position,
                ));
                break;
            }
        }
        if is_season_keyword(&tokens[a]) && tokens[b].is_delimiter() && tokens[c].is_free() {
            let value = if tokens[c].number {
                Some(tokens[c].value.clone())
            } else {
                from_roman_number(&tokens[c].value).map(str::to_owned)
            };
            if let Some(value) = value {
                tokens[a].element = Some(ElementKind::Season);
                tokens[c].element = Some(ElementKind::Season);
                elements.push(Element::new(ElementKind::Season, value, tokens[c].position));
                break;
            }
        }
    }

    // `S2`, `S01-02`
    static SEASON: LazyLock<Regex> = LazyLock::new(|| regex(r"^S([0-9]{1,2})$"));
    let mut i = 0;
    while i < tokens.len() {
        if !tokens[i].is_free() {
            i += 1;
            continue;
        }
        let Some(captures) = SEASON.captures(&tokens[i].value) else {
            i += 1;
            continue;
        };
        let position = tokens[i].position + group_start(&captures, 1);
        let value = group(&captures, 1).unwrap_or_default().to_owned();
        tokens[i].element = Some(ElementKind::Season);
        elements.push(Element::new(ElementKind::Season, value, position));
        let next = i + 1;
        if next < tokens.len() && tokens[next].is_dash() && next + 1 < tokens.len() {
            let after = next + 1;
            if tokens[after].is_free() && tokens[after].number {
                tokens[after].element = Some(ElementKind::Season);
                elements.push(element_from(
                    ElementKind::Season,
                    &tokens[after],
                    None,
                    None,
                ));
                break;
            }
        }
        i += 1;
    }

    // `第2期`
    if elements.is_empty() {
        static COUNTER: LazyLock<Regex> = LazyLock::new(|| regex(r"^(?:第)?([0-9]{1,2})期$"));
        for token in tokens.iter_mut().filter(|t| t.is_free()) {
            if let Some(captures) = COUNTER.captures(&token.value) {
                let position = token.position + group_start(&captures, 1);
                let value = group(&captures, 1).unwrap_or_default().to_owned();
                token.element = Some(ElementKind::Season);
                elements.push(Element::new(ElementKind::Season, value, position));
                break;
            }
        }
    }
    elements
}

fn parse_part(tokens: &mut [Token]) -> Option<Element> {
    for i in 0..tokens.len() {
        if tokens[i].keyword_kind() != Some(KeywordKind::Part) {
            continue;
        }
        let Some(next) = find_next(tokens, i, is_not_delimiter) else {
            continue;
        };
        if !tokens[next].number {
            continue;
        }
        tokens[i].element = Some(ElementKind::Part);
        tokens[next].element = Some(ElementKind::Part);
        return Some(element_from(ElementKind::Part, &tokens[next], None, None));
    }
    None
}

fn parse_volume(tokens: &mut [Token]) -> Vec<Element> {
    static SINGLE: LazyLock<Regex> = LazyLock::new(|| regex(r"^([0-9]{1,4})(?:[vV]([0-9]))?$"));
    static MULTIPLE: LazyLock<Regex> = LazyLock::new(|| regex(r"^([0-9]{1,4})&([0-9]{1,4})$"));
    let mut elements = Vec::new();
    let mut start = 0;
    while let Some(volume) =
        (start..tokens.len()).find(|&i| tokens[i].keyword_kind() == Some(KeywordKind::Volume))
    {
        let next = find_next(tokens, volume, is_not_delimiter);
        let Some(next) = next.filter(|&n| tokens[n].is_free()) else {
            break;
        };
        // `01`, `01v2`
        if let Some(captures) = SINGLE.captures(&tokens[next].value) {
            tokens[volume].element = Some(ElementKind::Volume);
            tokens[next].element = Some(ElementKind::Volume);
            elements.push(element_from(
                ElementKind::Volume,
                &tokens[next],
                group(&captures, 1),
                None,
            ));
            if let Some(version) = group(&captures, 2) {
                let position = tokens[next].position + group_start(&captures, 2);
                elements.push(Element::new(ElementKind::ReleaseVersion, version, position));
            }
        }
        // `1&2` (unreachable in practice, as `&` is a delimiter; kept for parity)
        if let Some(captures) = MULTIPLE.captures(&tokens[next].value) {
            tokens[volume].element = Some(ElementKind::Volume);
            tokens[next].element = Some(ElementKind::Volume);
            elements.push(element_from(
                ElementKind::Volume,
                &tokens[next],
                group(&captures, 1),
                None,
            ));
            elements.push(element_from(
                ElementKind::Volume,
                &tokens[next],
                group(&captures, 2),
                None,
            ));
        }
        start = volume + 1;
    }
    elements
}

// --- Episode -----------------------------------------------------------------

static EPISODE: LazyLock<Regex> = LazyLock::new(|| {
    regex(concat!(
        r"^(?:S([0-9]{1,2})|([0-9]{1,2})x)?", // season
        r"[E#]?([0-9]{1,4})",                 // episode
        r"(?:[vV]([0-9]))?$",                 // version
    ))
});

/// Season, episode and version from a matched episode token.
fn push_episode_matches(captures: &Captures, token: &mut Token, elements: &mut Vec<Element>) {
    if let Some(season) = group(captures, 1) {
        let position = token.position + group_start(captures, 1);
        elements.push(Element::new(ElementKind::Season, season, position));
    } else if let Some(season) = group(captures, 2) {
        let position = token.position + group_start(captures, 2);
        elements.push(Element::new(ElementKind::Season, season, position));
    }
    token.element = Some(ElementKind::Episode);
    let position = token.position + group_start(captures, 3);
    elements.push(element_from(
        ElementKind::Episode,
        token,
        group(captures, 3),
        Some(position),
    ));
    if let Some(version) = group(captures, 4) {
        let position = token.position + group_start(captures, 4);
        elements.push(Element::new(ElementKind::ReleaseVersion, version, position));
    }
}

fn add_episode(
    tokens: &mut [Token],
    i: usize,
    value: Option<&str>,
    position: Option<usize>,
    elements: &mut Vec<Element>,
) {
    tokens[i].element = Some(ElementKind::Episode);
    elements.push(element_from(
        ElementKind::Episode,
        &tokens[i],
        value,
        position,
    ));
}

fn parse_episode(tokens: &mut [Token]) -> Vec<Element> {
    let mut elements = Vec::new();
    let len = tokens.len();

    // Episode patterns (`01`, `E01`, `S01E01`, `2x05`, `01v2`, `01-12`)
    {
        let is_episode_keyword = |t: &Token| t.keyword_kind() == Some(KeywordKind::Episode);
        let is_type_keyword = |t: &Token| {
            matches!(
                t.keyword_kind(),
                Some(KeywordKind::Type | KeywordKind::EpisodeType)
            )
        };
        let is_range_delimiter =
            |t: &Token| t.is_delimiter() && matches!(first_byte(&t.value), '-' | '~' | '&' | '+');

        for i in 0..len {
            if !tokens[i].is_free() {
                continue;
            }
            let value = tokens[i].value.clone();
            let Some(first) = EPISODE.captures(&value) else {
                continue;
            };
            // A plain number is only an episode after a keyword (`Episode 1`,
            // `OVA1`) or as the start of a range (`1-12`).
            let mut valid = !tokens[i].number;
            if let Some(prev) = find_prev(tokens, i, is_not_delimiter)
                && (is_episode_keyword(&tokens[prev])
                    || (is_type_keyword(&tokens[prev]) && tokens[prev].value != "Movie"))
            {
                valid = true;
            }
            let mut second: Option<(usize, String)> = None;
            let next = i + 1;
            if next < len && is_range_delimiter(&tokens[next]) && next + 1 < len {
                let candidate = tokens[next + 1].value.clone();
                if let Some(captures) = EPISODE.captures(&candidate) {
                    // Avoid `009-1`, `5-2`, etc.
                    let ascending = to_int(group(&first, 3).unwrap_or_default())
                        < to_int(group(&captures, 3).unwrap_or_default());
                    valid = valid || ascending;
                    second = Some((next + 1, candidate.clone()));
                }
            }
            if !valid {
                continue;
            }
            push_episode_matches(&first, &mut tokens[i], &mut elements);
            if let Some((j, candidate)) = second
                && let Some(captures) = EPISODE.captures(&candidate)
            {
                push_episode_matches(&captures, &mut tokens[j], &mut elements);
            }
        }
        if !elements.is_empty() {
            return elements;
        }
    }

    // Separated episodes (`8 & 10`, `1 ~ 12`, `01 of 24`)
    for i in 0..len {
        if !(tokens[i].is_free() && tokens[i].number) {
            continue;
        }
        let separator = find_from(tokens, i + 1, |t| {
            matches!(t.value.as_str(), "&" | "~" | "of")
        });
        if separator == len {
            continue;
        }
        if tokens[i + 1..separator].iter().any(is_not_delimiter) {
            continue;
        }
        let Some(next) = find_next(tokens, separator, is_not_delimiter) else {
            continue;
        };
        if !tokens[next].number {
            continue;
        }
        add_episode(tokens, i, None, None, &mut elements);
        if tokens[separator].value != "of" {
            add_episode(tokens, next, None, None, &mut elements);
        }
        return elements;
    }

    // Fractional episode (`07.5`). Only `.5`, since `Evangelion 1.11` or
    // `Tokyo Magnitude 8.0` are titles and `5.1` is a keyword.
    for i in 0..len.saturating_sub(2) {
        let (number, delimiter, fraction) = (&tokens[i], &tokens[i + 1], &tokens[i + 2]);
        if number.is_free()
            && number.number
            && delimiter.is_delimiter()
            && delimiter.value == "."
            && fraction.is_free()
            && fraction.value == "5"
        {
            let value = format!("{}{}{}", number.value, delimiter.value, fraction.value);
            add_episode(tokens, i, Some(&value), None, &mut elements);
            tokens[i + 1].element = Some(ElementKind::Episode);
            tokens[i + 2].element = Some(ElementKind::Episode);
            return elements;
        }
    }

    // Japanese counter (`第01話`)
    static COUNTER: LazyLock<Regex> = LazyLock::new(|| regex(r"^(?:第)?([0-9]{1,4})話$"));
    // Equivalent numbers (`01 (176)`); a single token can't hold brackets, so
    // this never matches, as in the C++.
    static EQUIVALENT: LazyLock<Regex> =
        LazyLock::new(|| regex(r"^([0-9]{1,4})\s*\(([0-9]{1,4})\)$"));
    for pattern in [&*COUNTER, &*EQUIVALENT] {
        for i in 0..len {
            if !tokens[i].is_free() {
                continue;
            }
            let value = tokens[i].value.clone();
            if let Some(captures) = pattern.captures(&value) {
                let position = tokens[i].position + group_start(&captures, 1);
                add_episode(
                    tokens,
                    i,
                    group(&captures, 1),
                    Some(position),
                    &mut elements,
                );
                return elements;
            }
        }
    }

    // Separated number (` - 08`), from the end
    for dash in (0..len).rev().filter(|&d| tokens[d].is_dash()) {
        let next = find_from(tokens, dash + 1, is_not_delimiter);
        if next < len && tokens[next].is_free() && tokens[next].number {
            add_episode(tokens, next, None, None, &mut elements);
            return elements;
        }
    }

    // Isolated number (`[12]`, `(2006)`)
    if let Some(i) = (1..len.saturating_sub(1)).find(|&i| is_isolated_number(tokens, i)) {
        add_episode(tokens, i, None, None, &mut elements);
        return elements;
    }

    // Partial episode (`4a`, `111C`)
    static PARTIAL: LazyLock<Regex> = LazyLock::new(|| regex(r"^[0-9]{1,4}[ABCabc]$"));
    for i in 0..len {
        if !(tokens[i].is_free() && PARTIAL.is_match(&tokens[i].value)) {
            continue;
        }
        if i > 1 && tokens[i - 2].value == "Ver1" && tokens[i].value == "1a" {
            continue; // `NieR:Automata Ver1.1a`
        }
        add_episode(tokens, i, None, None, &mut elements);
        return elements;
    }

    // First number (`01 - Title`)
    let starts_with_number = len > 0 && tokens[0].is_free() && tokens[0].number && {
        len <= 2
            || tokens[1].is_dash()
            || tokens[2].is_dash()
            || (tokens[1].value == "."
                && (is_space(first_byte(&tokens[2].value))
                    || tokens[2].element == Some(ElementKind::FileExtension)))
    };
    if starts_with_number {
        add_episode(tokens, 0, None, None, &mut elements);
        return elements;
    }

    // Last number
    let is_version_before = |tokens: &[Token], p: usize| {
        // `1.1`: a number followed by `.`
        tokens[p].number
            && tokens
                .get(p + 1)
                .is_some_and(|t| t.is_delimiter() && t.value == ".")
    };
    let is_version_after = |tokens: &[Token], n: usize| {
        // `3.33`: a number preceded by `.`
        tokens[n].number && n > 0 && tokens[n - 1].is_delimiter() && tokens[n - 1].value == "."
    };
    for i in (0..len).rev() {
        let token = &tokens[i];
        if !(token.is_free() && token.number) || token.enclosed || token.position == 0 {
            continue;
        }
        let prev = find_prev(tokens, i, is_not_delimiter);
        // The C++ starts looking two tokens ahead; kept for parity.
        let next = (i + 2..len).find(|&n| is_not_delimiter(&tokens[n]));
        if let Some(p) = prev {
            let value = &tokens[p].value;
            let skip = ["Cour", "Part", "Movie", "No"]
                .iter()
                .any(|w| value.eq_ignore_ascii_case(w));
            if skip
                || is_version_before(tokens, p)
                || (tokens[p].is_close_bracket() && value == "]")
            {
                continue;
            }
        }
        if let Some(n) = next
            && is_version_after(tokens, n)
        {
            continue;
        }
        if let (Some(p), Some(n)) = (prev, next)
            && tokens[p].is_free()
            && tokens[n].is_free()
        {
            continue;
        }
        add_episode(tokens, i, None, None, &mut elements);
        return elements;
    }
    elements
}

// --- Title, release group, episode title -----------------------------------

fn find_title(tokens: &[Token]) -> Option<std::ops::Range<usize>> {
    let len = tokens.len();
    // Filenames starting with the episode number have no title.
    if tokens.first()?.element == Some(ElementKind::Episode) {
        return None;
    }
    // The first free range outside brackets: `[Group] Title - Episode [Info]`
    let mut first = find_from(tokens, 0, |t| t.is_free() && !t.enclosed);
    let mut last = find_from(tokens, first, Token::is_identified);
    // Or the second bracketed range: `[Group][Title][Info]`
    if first == len {
        first = find_from(tokens, 0, Token::is_close_bracket);
        first = find_from(tokens, first, Token::is_free);
        last = find_from(tokens, first, Token::is_bracket);
    }
    if first == len {
        return None;
    }
    // Mismatched brackets: `Title (` -> `Title `
    let opens: Vec<usize> = (first..last)
        .filter(|&i| tokens[i].is_open_bracket())
        .collect();
    if let Some(&last_open) = opens.last() {
        let closes = (first..last)
            .filter(|&i| tokens[i].is_close_bracket())
            .count();
        if closes != opens.len() {
            last = last_open;
        }
    }
    // No trailing brackets except parentheses: `Title [Group]` -> `Title `
    if let Some(token) = find_prev(tokens, last, is_not_delimiter)
        && tokens[token].is_close_bracket()
        && tokens[token].value != ")"
        && let Some(open) = find_prev(tokens, token, Token::is_open_bracket)
    {
        last = open;
    }
    (first < last).then_some(first..last)
}

fn parse_title(tokens: &mut [Token]) -> Option<Element> {
    let range = find_title(tokens)?;
    let value = value::build(&tokens[range.clone()], false);
    if value.is_empty() {
        return None;
    }
    let position = tokens[range.start].position;
    mark(tokens, range, ElementKind::Title);
    Some(Element::new(ElementKind::Title, value, position))
}

fn find_release_group(tokens: &[Token], offset: usize) -> std::ops::Range<usize> {
    let len = tokens.len();
    // The first bracketed range not yet identified: `[Group] Title`
    let mut first = find_from(tokens, 0, |t| t.enclosed && !t.is_identified());
    let mut last = find_from(tokens, first, |t| t.is_close_bracket() || t.is_identified());
    if first != len {
        // The range must be all there is between its brackets.
        if let Some(prev) = find_prev(tokens, first, is_not_delimiter)
            && !tokens[prev].is_open_bracket()
        {
            return find_release_group(&tokens[last..], offset + last);
        }
        if last != len && !tokens[last].is_close_bracket() {
            return find_release_group(&tokens[last..], offset + last);
        }
    }
    // Or the last token, after a dash: `Title.Episode.Info-Group.mkv`
    if first == len
        && let Some(token) = find_prev(tokens, len, |t| {
            t.element != Some(ElementKind::FileExtension) && !t.is_delimiter()
        })
        && tokens[token].is_free()
        && token > 0
        && tokens[token - 1].is_dash()
    {
        first = token;
        last = token + 1;
    }
    if first >= last {
        return 0..0;
    }
    offset + first..offset + last
}

fn parse_release_group(tokens: &mut [Token]) -> Option<Element> {
    let range = find_release_group(tokens, 0);
    if range.is_empty() {
        return None;
    }
    let value = value::build(&tokens[range.clone()], true);
    if value.is_empty() {
        return None;
    }
    let position = tokens[range.start].position;
    mark(tokens, range, ElementKind::ReleaseGroup);
    Some(Element::new(ElementKind::ReleaseGroup, value, position))
}

fn find_episode_title(tokens: &[Token]) -> Option<std::ops::Range<usize>> {
    let len = tokens.len();
    let episode = tokens
        .iter()
        .position(|t| t.element == Some(ElementKind::Episode))?;
    let is_invalid = |t: &Token| {
        t.is_identified()
            && !matches!(
                t.element,
                Some(ElementKind::Episode | ElementKind::ReleaseVersion | ElementKind::Season)
            )
    };
    // The first free range outside brackets after the episode:
    // `[Group] Title - Episode - Episode Title [Info]`
    let mut first = find_from(tokens, episode, |t| t.is_free() && !t.enclosed);
    if tokens[episode..first.min(len)].iter().any(is_invalid) {
        first = len;
    }
    let mut last = find_from(tokens, first, |t| {
        t.is_open_bracket() || (t.is_identified() && t.element != Some(ElementKind::Part))
    });
    // Or a range in corner brackets: `Title - Episode 「Episode Title」`
    if first == len {
        first = find_from(tokens, episode, |t| t.is_open_bracket() && t.value == "「");
        if first != len {
            first += 1;
        }
        last = find_from(tokens, first, |t| t.is_close_bracket() && t.value == "」");
        if last == len || tokens[first..last].iter().any(Token::is_identified) {
            return None;
        }
    }
    (first < last).then_some(first..last)
}

fn parse_episode_title(tokens: &mut [Token]) -> Option<Element> {
    let range = find_episode_title(tokens)?;
    let value = value::build(&tokens[range.clone()], false);
    // No empty or single-character episode titles.
    if value.len() <= 1 {
        return None;
    }
    let position = tokens[range.start].position;
    mark(tokens, range, ElementKind::EpisodeTitle);
    Some(Element::new(ElementKind::EpisodeTitle, value, position))
}
