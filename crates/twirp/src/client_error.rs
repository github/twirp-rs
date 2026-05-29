//! Client-specific error type that cleanly separates transport errors from Twirp application errors.

use thiserror::Error;

use crate::{GenericError, TwirpErrorResponse};

/// Error type returned by Twirp client calls.
///
/// Unlike [`TwirpErrorResponse`], which represents a well-formed Twirp error from the server,
/// `ClientError` distinguishes between transport-level failures and application-level Twirp errors.
/// This prevents transport errors from being shoehorned into Twirp error codes, and prevents
/// accidental `?`-propagation of backend Twirp errors in service-to-service calls.
#[derive(Debug, Error)]
pub enum ClientError {
    /// The HTTP request could not be completed (connect failure, timeout, DNS, TLS, etc.)
    #[error("transport error: {0}")]
    Transport(reqwest::Error),

    /// The response body could not be decoded (bad protobuf, malformed JSON, unexpected content type, etc.)
    #[error("invalid response: {0}")]
    InvalidResponse(GenericError),

    /// The request URL could not be constructed.
    #[error("invalid url: {0}")]
    InvalidUrl(url::ParseError),

    /// The server returned a well-formed Twirp error response.
    #[error(transparent)]
    Twirp(TwirpErrorResponse),
}

impl ClientError {
    /// Returns the Twirp error response if this is a `Twirp` variant.
    pub fn twirp_error(&self) -> Option<&TwirpErrorResponse> {
        match self {
            ClientError::Twirp(e) => Some(e),
            _ => None,
        }
    }

    /// Returns `true` if this error represents a transport-level failure.
    pub fn is_transport(&self) -> bool {
        matches!(self, ClientError::Transport(_))
    }

    /// Returns `true` if this error is a well-formed Twirp error from the server.
    pub fn is_twirp(&self) -> bool {
        matches!(self, ClientError::Twirp(_))
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(e: reqwest::Error) -> Self {
        ClientError::Transport(e)
    }
}

impl From<url::ParseError> for ClientError {
    fn from(e: url::ParseError) -> Self {
        ClientError::InvalidUrl(e)
    }
}

impl From<prost::DecodeError> for ClientError {
    fn from(e: prost::DecodeError) -> Self {
        ClientError::InvalidResponse(Box::new(e))
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(e: serde_json::Error) -> Self {
        ClientError::InvalidResponse(Box::new(e))
    }
}

impl From<TwirpErrorResponse> for ClientError {
    fn from(e: TwirpErrorResponse) -> Self {
        ClientError::Twirp(e)
    }
}
