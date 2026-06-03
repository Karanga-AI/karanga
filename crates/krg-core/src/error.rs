//! Error and Result types. The optimistic-concurrency `Stale` outcome is *not*
//! here — it is a normal [`crate::edit::WriteOut`] variant (interface §5).

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// Filesystem / archive I/O failure.
    Io(String),
    /// Malformed JSON or container.
    Parse(String),
    /// Violates the format, a schema, or the value domain (§9.1).
    Invalid(String),
    /// A referenced document or node does not exist.
    NotFound(String),
    /// Not implemented in this build.
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(m) => write!(f, "io error: {m}"),
            Error::Parse(m) => write!(f, "parse error: {m}"),
            Error::Invalid(m) => write!(f, "invalid: {m}"),
            Error::NotFound(m) => write!(f, "not found: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
        }
    }
}

impl std::error::Error for Error {}
