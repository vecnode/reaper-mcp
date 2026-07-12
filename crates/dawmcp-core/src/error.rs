use thiserror::Error;

#[derive(Debug, Error)]
pub enum DawError {
    #[error("DAW not reachable: {0}")]
    NotReachable(String),

    #[error("operation timed out after {0:?}")]
    Timeout(std::time::Duration),

    #[error("operation not supported by this DAW: {0}")]
    Unsupported(String),

    #[error("{0}")]
    Other(String),
}

pub type DawResult<T> = Result<T, DawError>;
