//! The player list (`players.anisthesia`), ported from Anisthesia's
//! `src/player.cpp`. Shown here with spaces; the file indents with tabs:
//!
//! ```text
//! mpv
//!     windows:
//!         mpv
//!     executables:
//!         mpv
//!     strategies:
//!         open_files
//!         window_title:
//!             ^No file - mpv|(.+) - mpv|mpv - (.+)
//! ```
//!
//! Values starting with `^` are regular expressions.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    WindowTitle,
    OpenFiles,
    UiAutomation,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerType {
    #[default]
    Default,
    WebBrowser,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub name: String,
    pub r#type: PlayerType,
    pub window_title_format: String,
    pub windows: Vec<String>,
    pub executables: Vec<String>,
    pub strategies: Vec<Strategy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: &'static str,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "players data, line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    PlayerName,
    Section,
    Window,
    Executable,
    Strategy,
    Type,
    WindowTitle,
}

impl State {
    fn indentation(self) -> usize {
        match self {
            Self::PlayerName => 0,
            Self::Section => 1,
            Self::Window | Self::Executable | Self::Strategy | Self::Type => 2,
            Self::WindowTitle => 3,
        }
    }
}

pub fn parse_players(data: &str) -> Result<Vec<Player>, ParseError> {
    let mut players: Vec<Player> = Vec::new();
    let mut state = State::PlayerName;
    for (index, raw) in data.split('\n').enumerate() {
        let error = |message| ParseError {
            line: index + 1,
            message,
        };
        if raw.is_empty() {
            continue;
        }
        let indentation = raw.len() - raw.trim_start_matches('\t').len();
        let line = raw.trim_start_matches('\t').trim_end_matches(['\n', '\r']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Each state expects a fixed indentation; less ends the section.
        let expected = state.indentation();
        if indentation > expected {
            return Err(error("too much indentation"));
        }
        if indentation < expected {
            let player = players.last();
            let section_empty = match state {
                State::Window => player.is_none_or(|p| p.windows.is_empty()),
                State::Executable => player.is_none_or(|p| p.executables.is_empty()),
                State::Strategy => player.is_none_or(|p| p.strategies.is_empty()),
                State::Type => false,
                // A window title format must follow `window_title:`.
                State::WindowTitle => return Err(error("missing window title format")),
                // The C++ leaves the state as it is here, so the line is then
                // read as a section name.
                State::PlayerName | State::Section => false,
            };
            if section_empty {
                return Err(error("empty section"));
            }
            if matches!(
                state,
                State::Window | State::Executable | State::Strategy | State::Type
            ) {
                state = if indentation == 0 {
                    State::PlayerName
                } else {
                    State::Section
                };
            }
        }

        match state {
            State::PlayerName => {
                players.push(Player {
                    name: line.to_owned(),
                    ..Player::default()
                });
                state = State::Section;
            }
            State::Section => {
                state = match line.trim_end_matches(':') {
                    "windows" => State::Window,
                    "executables" => State::Executable,
                    "strategies" => State::Strategy,
                    "type" => State::Type,
                    _ => return Err(error("unknown section")),
                };
            }
            State::Window => players.last_mut().unwrap().windows.push(line.to_owned()),
            State::Executable => players
                .last_mut()
                .unwrap()
                .executables
                .push(line.to_owned()),
            State::Strategy => {
                let strategy = match line.trim_end_matches(':') {
                    "window_title" => Strategy::WindowTitle,
                    "open_files" => Strategy::OpenFiles,
                    "ui_automation" => Strategy::UiAutomation,
                    _ => return Err(error("unknown strategy")),
                };
                players.last_mut().unwrap().strategies.push(strategy);
                if strategy == Strategy::WindowTitle {
                    state = State::WindowTitle;
                }
            }
            State::Type => {
                players.last_mut().unwrap().r#type = match line {
                    "default" => PlayerType::Default,
                    "web_browser" => PlayerType::WebBrowser,
                    _ => return Err(error("unknown type")),
                };
            }
            State::WindowTitle => {
                players.last_mut().unwrap().window_title_format = line.to_owned();
                state = State::Strategy;
            }
        }
    }
    if players.is_empty() {
        return Err(ParseError {
            line: 0,
            message: "no players",
        });
    }
    Ok(players)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bundled_file() {
        let data = include_str!("../../../src-tauri/resources/players.anisthesia");
        let players = parse_players(data).unwrap();
        // One per unindented, non-comment line.
        let names = data
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with(['\t', '#']))
            .count();
        assert_eq!(players.len(), names);
        assert!(players.len() > 40, "{}", players.len());

        let mpv = players.iter().find(|p| p.name == "mpv").unwrap();
        assert_eq!(mpv.windows, ["mpv"]);
        assert_eq!(mpv.executables, ["mpv"]);
        assert_eq!(mpv.strategies, [Strategy::OpenFiles, Strategy::WindowTitle]);
        assert_eq!(
            mpv.window_title_format,
            "^No file - mpv|(.+) - mpv|mpv - (.+)"
        );

        let chrome = players.iter().find(|p| p.name == "Google Chrome").unwrap();
        assert_eq!(chrome.r#type, PlayerType::WebBrowser);
        assert!(chrome.strategies.contains(&Strategy::UiAutomation));

        // Every regular expression compiles.
        for player in &players {
            for pattern in player
                .windows
                .iter()
                .chain(&player.executables)
                .filter(|p| p.starts_with('^'))
                .chain(std::iter::once(&player.window_title_format).filter(|f| !f.is_empty()))
            {
                assert!(
                    regex::Regex::new(pattern).is_ok(),
                    "{}: {pattern}",
                    player.name
                );
            }
        }
    }

    #[test]
    fn rejects_bad_indentation_and_sections() {
        assert!(parse_players("Player\n\t\t\tx\n").is_err());
        assert!(parse_players("Player\n\tunknown:\n").is_err());
        assert!(parse_players("Player\n\twindows:\nNext\n").is_err());
        assert!(parse_players("# only a comment\n").is_err());
    }

    #[test]
    fn handles_crlf_and_comments() {
        let data = "A\r\n\twindows:\r\n\t\tA\r\n\t\t# note\r\n\texecutables:\r\n\t\ta\r\n\tstrategies:\r\n\t\topen_files\r\n";
        let players = parse_players(data).unwrap();
        assert_eq!(players[0].windows, ["A"]);
        assert_eq!(players[0].strategies, [Strategy::OpenFiles]);
    }
}
