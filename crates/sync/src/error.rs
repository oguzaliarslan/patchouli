use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// The request didn't get a response.
    Network(reqwest::Error),
    /// The service rejected the token; the user must log in again.
    Unauthorized(String),
    /// The service returned an error.
    Api {
        status: u16,
        message: String,
    },
    /// The response wasn't in the expected format.
    Parse(String),
    /// A client ID or other build setting is missing.
    NotConfigured(String),
    /// Login failed or was cancelled.
    Auth(String),
    /// Tokens couldn't be read from or written to the credential store.
    Credentials(String),
    Database(tracker_db::Error),
    Io(std::io::Error),
    NotLoggedIn,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized(_))
    }

    pub(crate) fn parse(what: &str) -> Self {
        Self::Parse(format!("Could not parse {what}."))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(e) => write!(f, "Network error: {e}"),
            Self::Unauthorized(message) => write!(f, "{message}"),
            Self::Api { status, message } => write!(f, "{message} (HTTP {status})"),
            Self::Parse(message)
            | Self::NotConfigured(message)
            | Self::Auth(message)
            | Self::Credentials(message) => write!(f, "{message}"),
            Self::Database(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::NotLoggedIn => write!(f, "Not logged in."),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Network(e) => Some(e),
            Self::Database(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Self::Network(e)
    }
}

impl From<tracker_db::Error> for Error {
    fn from(e: tracker_db::Error) -> Self {
        Self::Database(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
