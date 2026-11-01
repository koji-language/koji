use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum KojiError {
    LexerError(String),
    ParserError(String),
    TypeError(String),
    RuntimeError(String),
    ImportError(String),
    /// An `Err` or `None` propagated with `?` out of the main file.
    UnhandledError(String),
}

impl fmt::Display for KojiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KojiError::LexerError(message) => write!(f, "syntax error: {}", message),
            KojiError::ParserError(message) => write!(f, "syntax error: {}", message),
            KojiError::TypeError(message) => write!(f, "type error: {}", message),
            KojiError::RuntimeError(message) => write!(f, "panic: {}", message),
            KojiError::ImportError(message) => write!(f, "import error: {}", message),
            KojiError::UnhandledError(message) => write!(f, "error: {}", message),
        }
    }
}

impl std::error::Error for KojiError {}
