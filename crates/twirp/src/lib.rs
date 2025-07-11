pub mod client;
pub mod error;
pub mod headers;
pub mod server;

#[cfg(any(test, feature = "test-support"))]
pub mod test;

#[doc(hidden)]
pub mod details;

pub use client::{Client, ClientBuilder, ClientError, Middleware, Next, Result};
pub use error::*; // many constructors like `invalid_argument()`
pub use http::Extensions;

// Re-export this crate's dependencies that users are likely to code against. These can be used to
// import the exact versions of these libraries `twirp` is built with -- useful if your project is
// so sprawling that it builds multiple versions of some crates.
pub use async_trait;
pub use axum;
pub use reqwest;
pub use tower;
pub use url;

/// Re-export of `axum::Router`, the type that encapsulates a server-side implementation of a Twirp
/// service.
pub use axum::Router;

pub(crate) fn serialize_proto_message<T>(m: T) -> Vec<u8>
where
    T: prost::Message,
{
    let len = m.encoded_len();
    let mut data = Vec::with_capacity(len);
    m.encode(&mut data)
        .expect("can only fail if buffer does not have capacity");
    assert_eq!(data.len(), len);
    data
}

#[derive(Debug, Default)]
pub struct Request<T>
where
    T: prost::Message + Default + serde::de::DeserializeOwned,
{
    pub inner: http::Request<T>,
}

impl<T> Request<T>
where
    T: prost::Message + Default + serde::de::DeserializeOwned,
{
    pub fn new(data: T) -> Self {
        Request {
            inner: http::Request::new(data),
        }
    }
}

#[derive(Debug, Default)]
pub struct Response<T>
where
    T: prost::Message + Default + serde::Serialize,
{
    pub inner: http::Response<T>,
}

impl<T> Response<T>
where
    T: prost::Message + Default + serde::Serialize,
{
    pub fn new(data: T) -> Self {
        Response {
            inner: http::Response::new(data),
        }
    }

    pub fn from_parts(parts: http::response::Parts, data: T) -> Self {
        Response {
            inner: http::Response::from_parts(parts, data),
        }
    }
}
