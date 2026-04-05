//! Crate-wide error type for boundaries that still box into [`std::error::Error`] (e.g. Stratum `EventHandler`).

use crate::share_handler::SubmitRunError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error(transparent)]
    Submit(#[from] SubmitRunError),
}

impl BridgeError {
    pub fn into_boxed_stratum(self) -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(self)
    }
}

#[cfg(test)]
mod bridge_error_tests {
    use super::BridgeError;
    use crate::share_handler::SubmitRunError;

    #[test]
    fn submit_variant_boxes() {
        let e: BridgeError = SubmitRunError::ReplyFailed("rpc".into()).into();
        assert!(!e.to_string().is_empty());
        let _ = e.into_boxed_stratum();
    }
}
