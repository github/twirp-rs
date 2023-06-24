//! Implement [Twirp](https://twitchtv.github.io/twirp/) error responses

use std::collections::HashMap;

use hyper::{header, Body, Response, StatusCode};
use serde::{Deserialize, Serialize, Serializer};

// Alias for a generic error
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

        $(
        pub fn $phrase<T: ToString>(msg: T) -> TwirpErrorResponse {
            TwirpErrorResponse {
                code: TwirpErrorCode::$konst,
                msg: msg.to_string(),
                meta: Default::default(),
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

// Twirp error responses are always JSON
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TwirpErrorResponse {
    pub code: TwirpErrorCode,
    pub msg: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(default)]
    pub meta: HashMap<String, String>,
}

impl TwirpErrorResponse {
    pub fn insert_meta(&mut self, key: String, value: String) -> Option<String> {
        self.meta.insert(key, value)
    }

    pub fn to_response(&self) -> Result<Response<Body>, GenericError> {
        let json = serde_json::to_string(self)?;
        let response = Response::builder()
            .status(self.code.http_status_code())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))?;
        Ok(response)
    }
}

#[cfg(test)]
mod test {
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
        let response = TwirpErrorResponse {
            code: TwirpErrorCode::DeadlineExceeded,
            msg: "test".to_string(),
            meta: Default::default(),
        };

        let result = serde_json::to_string(&response).unwrap();
        assert!(result.contains(r#""code":"deadline_exceeded""#));
        assert!(result.contains(r#""msg":"test""#));

        let result = serde_json::from_str(&result).unwrap();
        assert_eq!(response, result);
    }
}
