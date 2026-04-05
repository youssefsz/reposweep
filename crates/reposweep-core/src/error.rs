use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoSweepError {
    #[error("{operation} failed for {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("delete failed for {path}: {message}")]
    Delete { path: PathBuf, message: String },
    #[error("background worker error: {0}")]
    Worker(String),
}

pub type Result<T> = std::result::Result<T, RepoSweepError>;

impl RepoSweepError {
    pub fn io(operation: &'static str, path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }
}
