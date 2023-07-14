use hyper::header::{InvalidHeaderValue, CONTENT_TYPE};
use hyper::http::HeaderValue;
use hyper::{HeaderMap, StatusCode};
use reqwest::RequestBuilder;
use thiserror::Error;
use url::Url;

use crate::headers::*;
use crate::{error::*, to_proto_body};

#[derive(Debug, Error)]
pub enum TwirpClientError {
    #[error(transparent)]
    InvalidHeader(#[from] InvalidHeaderValue),
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error("http error, status code: {status}, msg:{msg}")]
    HttpError { status: StatusCode, msg: String },
    #[error(transparent)]
    JsonDecodeError(#[from] serde_json::Error),
    #[error("malformed response: {0}")]
    MalformedResponse(String),
    #[error(transparent)]
    ProtoDecodeError(#[from] prost::DecodeError),
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error("twirp error: {0:?}")]
    TwirpError(TwirpErrorResponse),
}

pub type Result<T> = core::result::Result<T, TwirpClientError>;

pub async fn request<I, O>(req: RequestBuilder, body: I) -> Result<O>
where
    I: prost::Message,
    O: prost::Message + Default,
{
    // eprintln!("{req:?}");
    let res = req.body(to_proto_body(body)).send().await?;

    // These have to be extracted because reading the body consumes `Response`.
    let status = res.status();
    let content_type = res.headers().get(CONTENT_TYPE).cloned();

    // eprintln!("{status:?} {content_type:?}");
    match (status, content_type) {
        (status, Some(ct)) if status.is_success() && ct.as_bytes() == CONTENT_TYPE_PROTOBUF => {
            O::decode(res.bytes().await?).map_err(|e| e.into())
        }
        (status, Some(ct)) if status.is_server_error() && ct.as_bytes() == CONTENT_TYPE_JSON => {
            Err(TwirpClientError::TwirpError(serde_json::from_slice(
                &res.bytes().await?,
            )?))
        }
        (status, None) if status.is_client_error() => Err(TwirpClientError::HttpError {
            status,
            msg: "client error".to_string(),
        }),
        (_, _) => Err(TwirpClientError::HttpError {
            status,
            msg: "unknown error".to_string(),
        }),
    }
}

#[derive(Clone, Debug)]
pub struct TwirpClient {
    pub client: reqwest::Client,
    pub base_url: Url,
}

impl TwirpClient {
    /// Creates a TwirpClient with the default `reqwest::ClientBuilder`.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn default(base_url: Url) -> Result<Self> {
        Self::new(base_url, reqwest::ClientBuilder::default())
    }

    /// Creates a TwirpClient.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn new(base_url: Url, b: reqwest::ClientBuilder) -> Result<Self> {
        let mut headers: HeaderMap<HeaderValue> = HeaderMap::default();
        headers.insert(CONTENT_TYPE, CONTENT_TYPE_PROTOBUF.try_into()?);
        let client = b.default_headers(headers).build()?;
        Ok(TwirpClient { base_url, client })
    }
}

#[cfg(test)]
mod tests {
    use crate::test::*;

    use super::*;

    #[tokio::test]
    #[ignore = "integration"]
    async fn test_standard_client() {
        let _ = run_test_server(3001).await;
        let base_url = Url::parse("http://localhost:3001/").unwrap();
        let client = TwirpClient::default(base_url).unwrap();
        let resp = client
            .ping(PingRequest {
                name: "hi".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(&resp.name, "hi");
    }

    #[tokio::test]
    #[ignore = "integration"]
    async fn test_custom_client() {
        let _ = run_test_server(3001).await;
        let base_url = Url::parse("http://example:3001").unwrap();
        let client = TwirpClient::default(base_url).unwrap();
        let client = TestAPIClientCustom {
            hmac_key: None,
            client,
        };
        let resp = client
            .ping(
                "localhost",
                PingRequest {
                    name: "hi".to_string(),
                },
            )
            .await
            .unwrap();
        assert_eq!(&resp.name, "hi");
    }
}
