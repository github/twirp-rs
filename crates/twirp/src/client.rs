use std::time::Duration;

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
    let res = req.body(to_proto_body(body)).send().await?;

    // These have to be extracted because reading the body consumes `Response`.
    let status = res.status();
    let content_type = res.headers().get(CONTENT_TYPE).cloned();

    match (status, content_type) {
        (status, Some(ct)) if status.is_success() && ct == CONTENT_TYPE_PROTOBUF => {
            O::decode(res.bytes().await?).map_err(|e| e.into())
        }
        (status, Some(ct)) if status.is_server_error() && ct == CONTENT_TYPE_JSON => Err(
            TwirpClientError::TwirpError(serde_json::from_slice(&res.bytes().await?)?),
        ),
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

pub struct TwirpClient {
    pub client: reqwest::Client,
    pub base_url: Url,
}

impl TwirpClient {
    /// Creates a TwirpClient.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn new(base_url: Url, user_agent: &str) -> Result<Self> {
        let mut headers: HeaderMap<HeaderValue> = HeaderMap::default();
        headers.insert(CONTENT_TYPE, CONTENT_TYPE_PROTOBUF.try_into()?);

        let client = reqwest::ClientBuilder::default()
            .connect_timeout(Duration::from_millis(500))
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(100)
            .default_headers(headers)
            .user_agent(user_agent)
            .build()?;
        Ok(TwirpClient { base_url, client })
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use crate::test::*;

    use super::*;

    #[tokio::test]
    async fn test_standard_client() {
        let base_url = Url::parse("http://example.com").unwrap();
        let client = TwirpClient::new(base_url, "test-ua").unwrap();
        let resp = client
            .ping(PingRequest {
                name: "hi".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(&resp.name, "hi");
    }

    // Custom client: add extra headers, do logging, etc
    pub struct TestAPIClientCustom {
        hmac_key: Option<String>,
        client: TwirpClient,
    }

    impl TestAPIClientCustom {
        async fn ping(&self, hostname: &str, req: PingRequest) -> Result<PingResponse> {
            let mut url = self.ping_url(&self.client.base_url)?;
            url.set_host(Some(hostname))?;
            self.ping_inner(url, req).await
        }
    }

    #[async_trait]
    impl TestAPIClientExt for TestAPIClientCustom {
        async fn ping_inner(&self, url: Url, req: PingRequest) -> Result<PingResponse> {
            let mut r = self
                .client
                .client
                .post(url)
                .header("X-GitHub-Request-Id", "XYZ");
            if let Some(_hmac_key) = &self.hmac_key {
                r = r.header("Request-HMAC", "example:todo");
            }
            request(r, req).await
        }
    }

    #[tokio::test]
    async fn test_custom_client() {
        let base_url = Url::parse("http://example.com").unwrap();
        let client = TwirpClient::new(base_url, "test-ua").unwrap();
        let client = TestAPIClientCustom {
            hmac_key: None,
            client,
        };
        let resp = client
            .ping(
                "hostname",
                PingRequest {
                    name: "hi".to_string(),
                },
            )
            .await
            .unwrap();
        assert_eq!(&resp.name, "hi");
    }
}
