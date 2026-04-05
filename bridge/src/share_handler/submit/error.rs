//! Typed errors for the `mining.submit` pipeline (`parse` → duplicate → PoW → finish).

use crate::stratum_context::ErrorDisconnected;
use thiserror::Error;

/// Synchronous validation / parsing failures before PoW runs.
#[derive(Debug, Error)]
pub enum SubmitError {
    #[error("malformed event, expected at least 3 params")]
    TooFewParams,
    #[error("job id must be a string or number")]
    JobIdWrongType,
    #[error("job id is not parsable as a number: {0}")]
    JobIdParse(String),
    #[error("job id number is out of range")]
    JobIdOutOfRange,
    #[error("job does not exist (stale)")]
    StaleJob,
    #[error("nonce must be a string")]
    NonceNotString,
    #[error("failed parsing nonce as hex: {0}")]
    NonceHexParse(String),
}

/// Full async submit flow: wraps parse errors, Stratum I/O, and reply failures.
#[derive(Debug, Error)]
pub enum SubmitRunError {
    #[error(transparent)]
    Validation(#[from] SubmitError),
    #[error(transparent)]
    StratumDisconnected(#[from] ErrorDisconnected),
    #[error("failed to send JSON-RPC reply: {0}")]
    ReplyFailed(String),
}
