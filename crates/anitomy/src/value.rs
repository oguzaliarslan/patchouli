//! Building element values from token ranges (`detail/element.hpp`).

use std::collections::BTreeSet;

use crate::chars::is_space;
use crate::token::Token;

/// Joins tokens into a value. Unless `keep_delimiters`, trailing delimiters are
/// trimmed and delimiters used as spaces (`_`, `.`, or the only one used)
/// become spaces.
pub fn build(tokens: &[Token], keep_delimiters: bool) -> String {
    let delimiters: BTreeSet<char> = tokens
        .iter()
        .filter(|t| t.is_delimiter())
        .map(Token::first_char)
        .collect();
    let single_delimiter = delimiters.len() == 1;
    let has_spaces = delimiters.iter().any(|&c| is_space(c));
    let has_underscores = delimiters.contains(&'_');

    let transformable = |token: &Token| {
        if keep_delimiters || !token.is_delimiter() {
            return false;
        }
        let ch = token.first_char();
        if matches!(ch, ',' | '&' | '~') {
            return false;
        }
        if is_space(ch) || ch == '_' {
            return true;
        }
        if has_spaces || has_underscores {
            return false;
        }
        if ch == '.' {
            return true;
        }
        single_delimiter
    };

    let mut tokens = tokens;
    if !keep_delimiters {
        let mut previous = '\0';
        while let Some(last) = tokens.last().filter(|t| t.is_delimiter()) {
            let delimiter = last.first_char();
            if delimiter == '~' || (delimiter == '.' && is_space(previous)) {
                break;
            }
            previous = delimiter;
            tokens = &tokens[..tokens.len() - 1];
        }
    }

    let mut value = String::new();
    for token in tokens {
        if transformable(token) {
            value.push(' ');
        } else {
            value.push_str(&token.value);
        }
    }
    value
}
