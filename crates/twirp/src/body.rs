use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use hyper::body::Frame;
use pin_project::pin_project;

use crate::GenericError;

type BoxBody = UnsyncBoxBody<Bytes, GenericError>;

/// Generic body type (like `axum::body::Body`).
#[pin_project]
pub struct Body(#[pin] BoxBody);

impl Debug for Body {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Body").finish()
    }
}

impl From<Bytes> for Body {
    fn from(bytes: Bytes) -> Self {
        Body(BoxBody::new(
            http_body_util::Full::new(bytes).map_err(|err| match err {}),
        ))
    }
}

impl From<Vec<u8>> for Body {
    fn from(bytes: Vec<u8>) -> Self {
        Bytes::from(bytes).into()
    }
}

impl From<String> for Body {
    fn from(text: String) -> Self {
        Bytes::from(text).into()
    }
}

impl From<&'static str> for Body {
    fn from(text: &'static str) -> Self {
        Bytes::from(text).into()
    }
}

impl Body {
    /// Create a new Body that wraps another `http::body::Body`.
    pub fn new<B>(body: B) -> Self
    where
        B: hyper::body::Body<Data = Bytes> + Send + 'static,
        B::Error: Into<GenericError>,
    {
        Body(BoxBody::new(body.map_err(|err| err.into())))
    }

    pub fn empty() -> Self {
        Self::new(http_body_util::Empty::new())
    }

    pub(crate) fn protobuf<T>(message: &T) -> Self
    where
        T: prost::Message,
    {
        serialize_proto_message(message).into()
    }

    pub(crate) fn json<T>(data: &T) -> Result<Self, serde_json::Error>
    where
        T: serde::Serialize,
    {
        let json = serde_json::to_string(&data)?;
        Ok(json.into())
    }
}

pub(crate) fn serialize_proto_message<T>(m: &T) -> Vec<u8>
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

impl hyper::body::Body for Body {
    /// Values yielded by the `Body`.
    type Data = bytes::Bytes;

    /// The error type this `Body` might generate.
    type Error = GenericError;

    /// Attempt to pull out the next data buffer of this stream.
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        self.project().0.poll_frame(cx)
    }
}
