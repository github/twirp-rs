use async_trait::async_trait;
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
    #[error("base_url must end in /, but got: {0}")]
    InvalidBaseUrl(Url),
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

    // TODO: Include more info in the error cases: request path, content-type, etc.
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

/// `TwirpClient` is the interface that Twirp clients must implement. See
/// `HttpTwirpClient` for the standard http client. You can define your own to
/// wrap requests or mock out APIs.
#[async_trait]
pub trait TwirpClient {
    async fn request<I, O>(&self, url: Url, body: I) -> Result<O>
    where
        I: prost::Message,
        O: prost::Message + Default;
}

/// `HttpTwirpClient` is a TwirpClient that uses `reqwest::Client` to make http
/// requests.
#[derive(Clone, Debug)]
pub struct HttpTwirpClient {
    pub client: reqwest::Client,
    pub base_url: Url,
}

impl HttpTwirpClient {
    /// Creates a TwirpClient with the default `reqwest::ClientBuilder`.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn default(base_url: Url) -> Result<Self> {
        if base_url.path().ends_with('/') {
            Self::new(base_url, reqwest::ClientBuilder::default())
        } else {
            Err(TwirpClientError::InvalidBaseUrl(base_url))
        }
    }

    /// Creates a TwirpClient.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn new(base_url: Url, b: reqwest::ClientBuilder) -> Result<Self> {
        let mut headers: HeaderMap<HeaderValue> = HeaderMap::default();
        headers.insert(CONTENT_TYPE, CONTENT_TYPE_PROTOBUF.try_into()?);
        let client = b.default_headers(headers).build()?;
        Ok(HttpTwirpClient { base_url, client })
    }
}

#[async_trait]
impl TwirpClient for HttpTwirpClient {
    async fn request<I, O>(&self, url: Url, body: I) -> Result<O>
    where
        I: prost::Message,
        O: prost::Message + Default,
    {
        request(self.client.post(url), body).await
    }
}

#[cfg(test)]
mod tests {
    use crate::test::*;

    use super::*;

    #[tokio::test]
    async fn test_base_url() {
        let url = Url::parse("http://localhost:3001/twirp/").unwrap();
        assert!(HttpTwirpClient::default(url).is_ok());
        let url = Url::parse("http://localhost:3001/twirp").unwrap();
        assert_eq!(
            HttpTwirpClient::default(url).unwrap_err().to_string(),
            "base_url must end in /, but got: http://localhost:3001/twirp",
        );
    }

    #[tokio::test]
    async fn test_routes() {
        let base_url = Url::parse("http://localhost:3001/twirp/").unwrap();
        let client = HttpTwirpClient::default(base_url.clone()).unwrap();
        assert_eq!(
            client.ping_url(&base_url).unwrap().to_string(),
            "http://localhost:3001/twirp/test.TestAPI/Ping"
        )
    }

    #[tokio::test]
    #[ignore = "integration"]
    async fn test_standard_client() {
        let h = run_test_server(3001).await;
        let base_url = Url::parse("http://localhost:3001/twirp/").unwrap();
        let client = HttpTwirpClient::default(base_url).unwrap();
        let resp = client
            .ping(PingRequest {
                name: "hi".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(&resp.name, "hi");
        h.abort()
    }

    #[tokio::test]
    #[ignore = "integration"]
    async fn test_custom_client() {
        let h = run_test_server(3001).await;
        let base_url = Url::parse("http://example:3001").unwrap();
        let client = HttpTwirpClient::default(base_url).unwrap();
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
        h.abort()
    }
}
