//! Implement [Twirp](https://twitchtv.github.io/twirp/) error responses

use std::collections::HashMap;

use hyper::{header, Body, Response, StatusCode};
use serde::{Serialize, Serializer};
use serde_json::Value;

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
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

// Define some of the most useful twirp errors.
//
// This is not an exhaustive list, feel free to add twirp error code mapping as
// needed.
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
    // The caller does not have permission to execute the specified operation.
    // It must not be used if the caller cannot be identified (use
    // "unauthenticated" instead).
    (PermissionDenied, StatusCode::FORBIDDEN, permission_denied);
    // The request does not have valid authentication credentials for the
    // operation.
    (Unauthenticated, StatusCode::UNAUTHORIZED, unauthenticated);
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
#[derive(Debug, Serialize)]
pub struct TwirpErrorResponse {
    pub(crate) code: TwirpErrorCode,
    pub(crate) msg: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub(crate) meta: HashMap<String, Value>,
}

impl TwirpErrorResponse {
    pub fn to_response(&self) -> Result<Response<Body>, GenericError> {
        let json = serde_json::to_string(self)?;
        let response = Response::builder()
            .status(self.code.http_status_code())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))?;
        Ok(response)
    }
}
