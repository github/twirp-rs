#[cfg(feature = "client")]
pub mod client;

pub mod error;
pub mod headers;
pub mod server;

#[cfg(any(test, feature = "test-support"))]
pub mod test;

// pub use client::*;
pub use error::*;
pub use server::*;

pub use url;

pub(crate) fn to_proto_body<T>(m: T) -> hyper::Body
where
    T: prost::Message,
{
    let len = m.encoded_len();
    let mut data = Vec::with_capacity(len);
    m.encode(&mut data)
        .expect("can only fail if buffer does not have capacity");
    assert_eq!(data.len(), len);
    hyper::Body::from(data)
}
