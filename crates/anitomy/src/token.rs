//! Tokens and the tokenizer (`detail/token.hpp`, `detail/tokenizer.hpp`).

use crate::ElementKind;
use crate::chars::{first_byte, is_close_bracket, is_dash, is_delimiter, is_open_bracket, is_text};
use crate::keyword::{KEYWORDS, Keyword};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    OpenBracket,
    CloseBracket,
    Delimiter,
    Keyword,
    Text,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub value: String,
    pub keyword: Option<Keyword>,
    pub element: Option<ElementKind>,
    /// Byte offset in the input.
    pub position: usize,
    /// Inside brackets.
    pub enclosed: bool,
    /// A text token made only of ASCII digits.
    pub number: bool,
}

impl Token {
    fn new(kind: TokenKind, value: String, keyword: Option<Keyword>) -> Self {
        Self {
            kind,
            value,
            keyword,
            element: None,
            position: 0,
            enclosed: false,
            number: false,
        }
    }

    pub fn is_identified(&self) -> bool {
        self.element.is_some()
    }
    pub fn is_free(&self) -> bool {
        matches!(self.kind, TokenKind::Text | TokenKind::Keyword) && self.element.is_none()
    }
    pub fn is_open_bracket(&self) -> bool {
        self.kind == TokenKind::OpenBracket
    }
    pub fn is_close_bracket(&self) -> bool {
        self.kind == TokenKind::CloseBracket
    }
    pub fn is_bracket(&self) -> bool {
        self.is_open_bracket() || self.is_close_bracket()
    }
    /// Only `-` qualifies: the C++ tests the first byte (see `first_byte`).
    pub fn is_dash(&self) -> bool {
        self.kind == TokenKind::Delimiter && is_dash(first_byte(&self.value))
    }
    pub fn is_delimiter(&self) -> bool {
        self.kind == TokenKind::Delimiter
    }
    pub fn is_keyword(&self) -> bool {
        self.kind == TokenKind::Keyword
    }
    pub fn is_text(&self) -> bool {
        self.kind == TokenKind::Text
    }
    pub fn keyword_kind(&self) -> Option<crate::keyword::KeywordKind> {
        self.keyword.map(|k| k.kind)
    }
    /// First code point of the value.
    pub fn first_char(&self) -> char {
        self.value.chars().next().unwrap_or('\0')
    }
}

/// Splits the input into brackets, delimiters, keywords and text.
pub fn tokenize(input: &str) -> Vec<Token> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let token = if is_open_bracket(ch) {
            i += 1;
            Token::new(TokenKind::OpenBracket, ch.to_string(), None)
        } else if is_close_bracket(ch) {
            i += 1;
            Token::new(TokenKind::CloseBracket, ch.to_string(), None)
        } else if is_delimiter(ch) {
            i += 1;
            Token::new(TokenKind::Delimiter, ch.to_string(), None)
        } else if let Some((n, keyword)) = take_keyword(&chars[i..]) {
            let value: String = chars[i..i + n].iter().collect();
            i += n;
            Token::new(TokenKind::Keyword, value, Some(keyword))
        } else {
            let n = chars[i..]
                .iter()
                .take_while(|&&c| is_text(c))
                .count()
                .max(1);
            let value: String = chars[i..i + n].iter().collect();
            i += n;
            Token::new(TokenKind::Text, value, None)
        };
        tokens.push(token);
    }

    let mut level = 0i32;
    let mut position = 0;
    for token in &mut tokens {
        match token.kind {
            TokenKind::OpenBracket => level += 1,
            TokenKind::CloseBracket => level -= 1,
            _ => token.enclosed = level > 0,
        }
        token.position = position;
        position += token.value.len();
        if token.kind == TokenKind::Text {
            token.number = token.value.bytes().all(|b| b.is_ascii_digit());
        }
    }
    tokens
}

/// The longest keyword at the start of `view` (in the input's case), if any.
fn find_key(view: &[char]) -> String {
    let mut key = String::new();
    let mut prefix = String::new();
    for &ch in view {
        prefix.push(ch);
        if KEYWORDS.get(&prefix).is_some() {
            key.clone_from(&prefix);
        }
        if !KEYWORDS.has_candidates(&prefix) {
            break;
        }
    }
    key
}

fn take_keyword(view: &[char]) -> Option<(usize, Keyword)> {
    let key = find_key(view);
    if key.is_empty() {
        return None;
    }
    let keyword = KEYWORDS.get(&key)?;
    // The C++ counts the key's bytes but then takes that many code points,
    // which differs only for non-ASCII keys (`Episódio`). Kept for parity.
    let n = key.len().min(view.len());
    let rest = &view[n..];
    // A keyword must end at a word boundary, unless it's a subword (`1080p`)
    // or a prefix followed by a number (`Ep01`) or another keyword (`AAC2.0`).
    let boundary = if keyword.is_subword() || rest.is_empty() || !is_text(rest[0]) {
        true
    } else if keyword.is_prefix_for_number() {
        rest[0].is_ascii_digit()
    } else if keyword.is_prefix_for_other() {
        !find_key(rest).is_empty()
    } else {
        false
    };
    boundary.then_some((n, keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<(TokenKind, String)> {
        tokenize(input)
            .into_iter()
            .map(|t| (t.kind, t.value))
            .collect()
    }

    #[test]
    fn splits_brackets_delimiters_keywords_and_text() {
        use TokenKind::*;
        assert_eq!(
            kinds("[Group] Title - 01 [1080p].mkv"),
            [
                (OpenBracket, "[".into()),
                (Text, "Group".into()),
                (CloseBracket, "]".into()),
                (Delimiter, " ".into()),
                (Text, "Title".into()),
                (Delimiter, " ".into()),
                (Delimiter, "-".into()),
                (Delimiter, " ".into()),
                (Text, "01".into()),
                (Delimiter, " ".into()),
                (OpenBracket, "[".into()),
                (Keyword, "1080p".into()),
                (CloseBracket, "]".into()),
                (Delimiter, ".".into()),
                (Text, "mkv".into()),
            ]
        );
    }

    #[test]
    fn keywords_need_a_boundary() {
        // `Ep` is a prefix for numbers, so `Ep01` splits but `Epic` doesn't.
        assert_eq!(kinds("Ep01")[0], (TokenKind::Keyword, "Ep".into()));
        assert_eq!(kinds("Epic")[0], (TokenKind::Text, "Epic".into()));
        // Multi-word keywords match with any of the variant delimiters.
        assert_eq!(
            kinds("Dual.Audio")[0],
            (TokenKind::Keyword, "Dual.Audio".into())
        );
    }

    #[test]
    fn positions_are_byte_offsets() {
        let tokens = tokenize("第01話 x");
        assert_eq!(tokens.last().unwrap().position, "第01話 ".len());
    }
}
