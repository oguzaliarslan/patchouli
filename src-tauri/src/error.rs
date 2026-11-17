use serde::Serialize;

/// Error returned by commands. It reaches the frontend as a plain message.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct CommandError(String);

impl<E: std::error::Error> From<E> for CommandError {
    fn from(e: E) -> Self {
        Self(e.to_string())
    }
}

pub type CommandResult<T> = Result<T, CommandError>;
