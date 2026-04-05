//! Typed errors at the Stratum [`crate::stratum_listener::EventHandler`] boundary (boxed as trait objects).

use crate::share_handler::SubmitRunError;
use thiserror::Error;

/// Preserves structured [`SubmitRunError`] instead of stringifying into [`std::io::Error`].
#[derive(Debug, Error)]
pub(crate) enum StratumHandlerError {
    #[error(transparent)]
    Submit(#[from] SubmitRunError),
}

impl StratumHandlerError {
    pub(crate) fn into_boxed(self) -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(self)
    }
}
