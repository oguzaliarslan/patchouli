use serde::Serialize;

/// Error returned by commands. The frontend receives `{ code, message }`;
/// `code` is set for errors it handles specially.
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: Option<&'static str>,
    pub message: String,
}

impl CommandError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
        }
    }

    pub fn with_code(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            message: message.into(),
        }
    }
}

impl<E: std::error::Error> From<E> for CommandError {
    fn from(e: E) -> Self {
        Self::new(e.to_string())
    }
}

pub type CommandResult<T> = Result<T, CommandError>;
