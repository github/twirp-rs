//! Implement [Twirp](https://twitchtv.github.io/twirp/) error responses

use std::collections::HashMap;
use std::time::Duration;

use axum::body::Body;
use axum::response::IntoResponse;
use http::header::{self};
use hyper::{Response, StatusCode};
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

/// Alias for a generic error
pub type GenericError = Box<dyn std::error::Error + Send + Sync>;

macro_rules! twirp_error_codes {
    (
        $(
            $(#[$docs:meta])*
            ($konst:ident, $num:expr, $phrase:ident);
        )+
    ) => {
        /// A Twirp error code as defined by <https://twitchtv.github.io/twirp/docs/spec_v7.html>.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        #[non_exhaustive]
        pub enum TwirpErrorCode {
            $(
                $(#[$docs])*
                $konst,
            )+
        }

        impl TwirpErrorCode {
            pub fn http_status_code(&self) -> StatusCode {
                match *self {
                    $(
                        TwirpErrorCode::$konst => $num,
                    )+
                }
            }

            pub fn twirp_code(&self) -> &'static str {
                match *self {
                    $(
                        TwirpErrorCode::$konst => stringify!($phrase),
                    )+
                }
            }
        }

        impl From<StatusCode> for TwirpErrorCode {
            fn from(code: StatusCode) -> Self {
                $(
                    if code == $num {
                        return TwirpErrorCode::$konst;
                    }
                )+
                return TwirpErrorCode::Unknown
            }
        }

        $(
        pub fn $phrase<T: ToString>(msg: T) -> TwirpErrorResponse {
            TwirpErrorResponse {
                code: TwirpErrorCode::$konst,
                msg: msg.to_string(),
                meta: Default::default(),
                rust_error: None,
                retry_after: None,
            }
        }
        )+
    }
}

// Define all twirp errors.
twirp_error_codes! {
    /// The operation was cancelled.
    (Canceled, StatusCode::REQUEST_TIMEOUT, canceled);
    /// An unknown error occurred. For example, this can be used when handling
    /// errors raised by APIs that do not return any error information.
    (Unknown, StatusCode::INTERNAL_SERVER_ERROR, unknown);
    /// The client specified an invalid argument. This indicates arguments that
    /// are invalid regardless of the state of the system (i.e. a malformed file
    /// name, required argument, number out of range, etc.).
    (InvalidArgument, StatusCode::BAD_REQUEST, invalid_argument);
    /// The client sent a message which could not be decoded. This may mean that
    /// the message was encoded improperly or that the client and server have
    /// incompatible message definitions.
    (Malformed, StatusCode::BAD_REQUEST, malformed);
    /// Operation expired before completion. For operations that change the
    /// state of the system, this error may be returned even if the operation
    /// has completed successfully (timeout).
    (DeadlineExceeded,  StatusCode::REQUEST_TIMEOUT, deadline_exceeded);
    /// Some requested entity was not found.
    (NotFound, StatusCode::NOT_FOUND, not_found);
    /// The requested URL path wasn't routable to a Twirp service and method.
    /// This is returned by generated server code and should not be returned by
    /// application code (use "not_found" or "unimplemented" instead).
    (BadRoute, StatusCode::NOT_FOUND, bad_route);
    /// An attempt to create an entity failed because one already exists.
    (AlreadyExists, StatusCode::CONFLICT, already_exists);
    // The caller does not have permission to execute the specified operation.
    // It must not be used if the caller cannot be identified (use
    // "unauthenticated" instead).
    (PermissionDenied, StatusCode::FORBIDDEN, permission_denied);
    // The request does not have valid authentication credentials for the
    // operation.
    (Unauthenticated, StatusCode::UNAUTHORIZED, unauthenticated);
    /// Some resource has been exhausted or rate-limited, perhaps a per-user
    /// quota, or perhaps the entire file system is out of space.
    (ResourceExhausted, StatusCode::TOO_MANY_REQUESTS, resource_exhausted);
    /// The operation was rejected because the system is not in a state required
    /// for the operation's execution. For example, doing an rmdir operation on
    /// a directory that is non-empty, or on a non-directory object, or when
    /// having conflicting read-modify-write on the same resource.
    (FailedPrecondition, StatusCode::PRECONDITION_FAILED, failed_precondition);
    /// The operation was aborted, typically due to a concurrency issue like
    /// sequencer check failures, transaction aborts, etc.
    (Aborted, StatusCode::CONFLICT, aborted);
    /// The operation was attempted past the valid range. For example, seeking
    /// or reading past end of a paginated collection. Unlike
    /// "invalid_argument", this error indicates a problem that may be fixed if
    /// the system state changes (i.e. adding more items to the collection).
    /// There is a fair bit of overlap between "failed_precondition" and
    /// "out_of_range". We recommend using "out_of_range" (the more specific
    /// error) when it applies so that callers who are iterating through a space
    /// can easily look for an "out_of_range" error to detect when they are
    /// done.
    (OutOfRange, StatusCode::BAD_REQUEST, out_of_range);
    /// The operation is not implemented or not supported/enabled in this
    /// service.
    (Unimplemented, StatusCode::NOT_IMPLEMENTED, unimplemented);
    /// When some invariants expected by the underlying system have been broken.
    /// In other words, something bad happened in the library or backend
    /// service. Twirp specific issues like wire and serialization problems are
    /// also reported as "internal" errors.
    (Internal, StatusCode::INTERNAL_SERVER_ERROR, internal);
    /// The service is currently unavailable. This is most likely a transient
    /// condition and may be corrected by retrying with a backoff.
    (Unavailable, StatusCode::SERVICE_UNAVAILABLE, unavailable);
    /// The operation resulted in unrecoverable data loss or corruption.
    (Dataloss, StatusCode::INTERNAL_SERVER_ERROR, dataloss);
}

impl Serialize for TwirpErrorCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.twirp_code())
    }
}

/// A Twirp error response meeting the spec: https://twitchtv.github.io/twirp/docs/spec_v7.html#error-codes.
///
/// NOTE: Twirp error responses are always sent as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Error)]
pub struct TwirpErrorResponse {
    /// One of the Twirp error codes.
    pub code: TwirpErrorCode,

    /// A human-readable message describing the error.
    pub msg: String,

    /// (Optional) An object with string values holding arbitrary additional metadata describing the error.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(default)]
    pub meta: HashMap<String, String>,

    /// (Optional) How long clients should wait before retrying. If set, will be included in the `Retry-After` response
    /// header. Generally only valid for HTTP 429 or 503 responses. NOTE: This is *not* technically part of the twirp
    /// spec.
    #[serde(skip_serializing)]
    retry_after: Option<Duration>,

    /// Debug form of the underlying Rust error (if any). NOT returned to clients.
    #[serde(skip_serializing)]
    rust_error: Option<String>,
}

impl TwirpErrorResponse {
    pub fn new(code: TwirpErrorCode, msg: String) -> Self {
        Self {
            code,
            msg,
            meta: HashMap::new(),
            rust_error: None,
            retry_after: None,
        }
    }

    pub fn http_status_code(&self) -> StatusCode {
        self.code.http_status_code()
    }

    pub fn meta_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.meta
    }

    pub fn with_meta<S: ToString>(mut self, key: S, value: S) -> Self {
        self.meta.insert(key.to_string(), value.to_string());
        self
    }

    pub fn retry_after(&self) -> Option<Duration> {
        self.retry_after
    }

    pub fn with_generic_error(self, err: GenericError) -> Self {
        self.with_rust_error_string(format!("{err:?}"))
    }

    pub fn with_rust_error<E: std::error::Error>(self, err: E) -> Self {
        self.with_rust_error_string(format!("{err:?}"))
    }

    pub fn with_rust_error_string(mut self, rust_error: String) -> Self {
        self.rust_error = Some(rust_error);
        self
    }

    pub fn with_retry_after(mut self, duration: impl Into<Option<Duration>>) -> Self {
        let duration = duration.into();
        self.retry_after = duration.map(|d| {
            // Ensure that the duration is at least 1 second, as per HTTP spec.
            if d.as_secs() < 1 {
                Duration::from_secs(1)
            } else {
                d
            }
        });
        if let Some(ref retry_after) = self.retry_after {
            self.meta
                .insert("retry_after".to_string(), retry_after.as_secs().to_string());
        } else {
            self.meta.remove("retry_after");
        }
        self
    }
}

/// Shorthand for an internal server error triggered by a Rust error.
pub fn internal_server_error<E: std::error::Error>(err: E) -> TwirpErrorResponse {
    internal("internal server error").with_rust_error(err)
}

// twirp response from server failed to decode
impl From<prost::DecodeError> for TwirpErrorResponse {
    fn from(e: prost::DecodeError) -> Self {
        internal(e.to_string())
    }
}

// twirp error response from server was invalid
impl From<serde_json::Error> for TwirpErrorResponse {
    fn from(e: serde_json::Error) -> Self {
        internal(e.to_string())
    }
}

// unable to build the request
impl From<reqwest::Error> for TwirpErrorResponse {
    fn from(e: reqwest::Error) -> Self {
        invalid_argument(e.to_string())
    }
}

// Failed modify the request url
impl From<url::ParseError> for TwirpErrorResponse {
    fn from(e: url::ParseError) -> Self {
        invalid_argument(e.to_string())
    }
}

// Invalid header value (client middleware examples use this)
impl From<header::InvalidHeaderValue> for TwirpErrorResponse {
    fn from(e: header::InvalidHeaderValue) -> Self {
        invalid_argument(e.to_string())
    }
}

impl From<anyhow::Error> for TwirpErrorResponse {
    fn from(err: anyhow::Error) -> Self {
        internal("internal server error").with_rust_error_string(format!("{err:#}"))
    }
}

impl IntoResponse for TwirpErrorResponse {
    fn into_response(self) -> Response<Body> {
        let mut resp = Response::builder()
            .status(self.http_status_code())
            // NB: Add this in the response extensions so that axum layers can extract (e.g. for logging)
            .extension(self.clone())
            .header(header::CONTENT_TYPE, "application/json");

        if let Some(duration) = self.retry_after {
            resp = resp.header(header::RETRY_AFTER, duration.as_secs().to_string());
        }

        let json = serde_json::to_string(&self)
            .expect("json serialization of a TwirpErrorResponse should not fail");
        resp.body(Body::new(json))
            .expect("failed to build TwirpErrorResponse")
    }
}

impl std::fmt::Display for TwirpErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "error {:?}: {}", self.code, self.msg)?;
        if !self.meta.is_empty() {
            write!(f, " (meta: {{")?;
            let mut first = true;
            for (k, v) in &self.meta {
                if !first {
                    write!(f, ", ")?;
                }
                write!(f, "{k:?}: {v:?}")?;
                first = false;
            }
            write!(f, "}})")?;
        }
        if let Some(ref retry_after) = self.retry_after {
            write!(f, " (retry_after: {:?})", retry_after)?;
        }
        if let Some(ref rust_error) = self.rust_error {
            write!(f, " (rust_error: {:?})", rust_error)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use crate::{TwirpErrorCode, TwirpErrorResponse};

    #[test]
    fn twirp_status_mapping() {
        assert_code(TwirpErrorCode::Canceled, "canceled", 408);
        assert_code(TwirpErrorCode::Unknown, "unknown", 500);
        assert_code(TwirpErrorCode::InvalidArgument, "invalid_argument", 400);
        assert_code(TwirpErrorCode::Malformed, "malformed", 400);
        assert_code(TwirpErrorCode::Unauthenticated, "unauthenticated", 401);
        assert_code(TwirpErrorCode::PermissionDenied, "permission_denied", 403);
        assert_code(TwirpErrorCode::DeadlineExceeded, "deadline_exceeded", 408);
        assert_code(TwirpErrorCode::NotFound, "not_found", 404);
        assert_code(TwirpErrorCode::BadRoute, "bad_route", 404);
        assert_code(TwirpErrorCode::Unimplemented, "unimplemented", 501);
        assert_code(TwirpErrorCode::Internal, "internal", 500);
        assert_code(TwirpErrorCode::Unavailable, "unavailable", 503);
    }

    fn assert_code(code: TwirpErrorCode, msg: &str, http: u16) {
        assert_eq!(
            code.http_status_code(),
            http,
            "expected http status code {} but got {}",
            http,
            code.http_status_code()
        );
        assert_eq!(
            code.twirp_code(),
            msg,
            "expected error message '{}' but got '{}'",
            msg,
            code.twirp_code()
        );
    }

    #[test]
    fn twirp_error_response_serialization() {
        let meta = HashMap::from([
            ("key1".to_string(), "value1".to_string()),
            ("key2".to_string(), "value2".to_string()),
        ]);
        let response = TwirpErrorResponse {
            code: TwirpErrorCode::DeadlineExceeded,
            msg: "test".to_string(),
            meta,
            rust_error: None,
            retry_after: None,
        };

        let result = serde_json::to_string(&response).unwrap();
        assert!(result.contains(r#""code":"deadline_exceeded""#));
        assert!(result.contains(r#""msg":"test""#));
        assert!(result.contains(r#""key1":"value1""#));
        assert!(result.contains(r#""key2":"value2""#));

        let result = serde_json::from_str(&result).unwrap();
        assert_eq!(response, result);
    }

    #[test]
    fn twirp_error_response_serialization_skips_fields() {
        let response = TwirpErrorResponse {
            code: TwirpErrorCode::Unauthenticated,
            msg: "test".to_string(),
            meta: HashMap::new(),
            rust_error: Some("not included".to_string()),
            retry_after: None,
        };

        let result = serde_json::to_string(&response).unwrap();
        assert!(result.contains(r#""code":"unauthenticated""#));
        assert!(result.contains(r#""msg":"test""#));
        assert!(!result.contains(r#"rust_error"#));
    }
}
