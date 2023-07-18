use std::sync::Arc;

use async_trait::async_trait;
use hyper::header::{InvalidHeaderValue, CONTENT_TYPE};
use hyper::http::HeaderValue;
use hyper::{HeaderMap, StatusCode};
use reqwest::ClientBuilder;
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
    #[error(
        "http error, status code: {status}, msg:{msg} for path:{path} and content-type:{content_type}"
    )]
    HttpError {
        status: StatusCode,
        msg: String,
        path: String,
        content_type: String,
    },
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

pub struct TwirpClientBuilder {
    base_url: Url,
    builder: ClientBuilder,
    middleware: Vec<Arc<dyn Middleware>>,
}

impl TwirpClientBuilder {
    pub fn new(base_url: Url) -> Self {
        Self {
            base_url,
            builder: ClientBuilder::default(),
            middleware: vec![],
        }
    }

    pub fn with<M>(self, middleware: M) -> Self
    where
        M: Middleware,
    {
        let mut mw = self.middleware.clone();
        mw.push(Arc::new(middleware));
        Self {
            base_url: self.base_url,
            builder: self.builder,
            middleware: mw,
        }
    }

    pub fn with_client_builder(self, builder: ClientBuilder) -> Self {
        Self {
            base_url: self.base_url,
            builder,
            middleware: self.middleware,
        }
    }

    pub fn build(self) -> Result<HttpTwirpClient> {
        HttpTwirpClient::new(self.base_url, self.builder, self.middleware)
    }
}

/// `HttpTwirpClient` is a TwirpClient that uses `reqwest::Client` to make http
/// requests.
#[derive(Clone)]
pub struct HttpTwirpClient {
    pub base_url: Arc<Url>,
    client: Arc<reqwest::Client>,
    middlewares: Vec<Arc<dyn Middleware>>,
}

impl std::fmt::Debug for HttpTwirpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TwirpClient")
            .field("base_url", &self.base_url)
            .field("client", &self.client)
            .field("middlewares", &self.middlewares.len())
            .finish()
    }
}

impl HttpTwirpClient {
    /// Creates a TwirpClient with the default `reqwest::ClientBuilder`.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn default(base_url: Url) -> Result<Self> {
        if base_url.path().ends_with('/') {
            Self::new(base_url, ClientBuilder::default(), vec![])
        } else {
            Err(TwirpClientError::InvalidBaseUrl(base_url))
        }
    }

    /// Creates a TwirpClient.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn new(
        base_url: Url,
        b: ClientBuilder,
        middlewares: Vec<Arc<dyn Middleware>>,
    ) -> Result<Self> {
        let mut headers: HeaderMap<HeaderValue> = HeaderMap::default();
        headers.insert(CONTENT_TYPE, CONTENT_TYPE_PROTOBUF.try_into()?);
        let client = b.default_headers(headers).build()?;
        Ok(HttpTwirpClient {
            base_url: Arc::new(base_url),
            client: Arc::new(client),
            middlewares,
        })
    }

    /// Add some middleware to the request stack.
    pub fn with<M>(&self, middleware: M) -> Self
    where
        M: Middleware,
    {
        let mut middlewares = self.middlewares.clone();
        middlewares.push(Arc::new(middleware));
        Self {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
            middlewares,
        }
    }

    pub async fn request<I, O>(&self, url: Url, body: I) -> Result<O>
    where
        I: prost::Message,
        O: prost::Message + Default,
    {
        let path = url.path().to_string();
        let req = self.client.post(url).body(to_proto_body(body)).build()?;

        // Create and execute the middleware handlers
        let next = Next::new(&self.client, &self.middlewares);
        let resp = next.run(req).await?;

        // These have to be extracted because reading the body consumes `Response`.
        let status = resp.status();
        let content_type = resp.headers().get(CONTENT_TYPE).cloned();

        // TODO: Include more info in the error cases: request path, content-type, etc.
        match (status, content_type) {
            (status, Some(ct)) if status.is_success() && ct.as_bytes() == CONTENT_TYPE_PROTOBUF => {
                O::decode(resp.bytes().await?).map_err(|e| e.into())
            }
            (status, Some(ct))
                if (status.is_client_error() || status.is_server_error())
                    && ct.as_bytes() == CONTENT_TYPE_JSON =>
            {
                Err(TwirpClientError::TwirpError(serde_json::from_slice(
                    &resp.bytes().await?,
                )?))
            }
            (status, ct) => Err(TwirpClientError::HttpError {
                status,
                msg: "unknown error".to_string(),
                path,
                content_type: ct
                    .map(|x| x.to_str().unwrap_or_default().to_string())
                    .unwrap_or_default(),
            }),
        }
    }
}

// This concept of reqwest middleware is taken pretty much directly from:
// https://github.com/TrueLayer/reqwest-middleware, but simplified for the
// specific needs of this twirp client.
#[async_trait]
pub trait Middleware: 'static + Send + Sync {
    async fn handle(&self, mut req: reqwest::Request, next: Next<'_>) -> Result<reqwest::Response>;
}

#[async_trait]
impl<F> Middleware for F
where
    F: Send
        + Sync
        + 'static
        + for<'a> Fn(reqwest::Request, Next<'a>) -> BoxFuture<'a, Result<reqwest::Response>>,
{
    async fn handle(&self, req: reqwest::Request, next: Next<'_>) -> Result<reqwest::Response> {
        (self)(req, next).await
    }
}

#[derive(Clone)]
pub struct Next<'a> {
    client: &'a reqwest::Client,
    middlewares: &'a [Arc<dyn Middleware>],
}

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

impl<'a> Next<'a> {
    pub(crate) fn new(client: &'a reqwest::Client, middlewares: &'a [Arc<dyn Middleware>]) -> Self {
        Next {
            client,
            middlewares,
        }
    }

    pub fn run(mut self, req: reqwest::Request) -> BoxFuture<'a, Result<reqwest::Response>> {
        if let Some((current, rest)) = self.middlewares.split_first() {
            self.middlewares = rest;
            Box::pin(current.handle(req, self))
        } else {
            Box::pin(async move {
                self.client
                    .execute(req)
                    .await
                    .map_err(TwirpClientError::from)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use reqwest::{Request, Response};

    use crate::test::*;

    use super::*;

    struct AssertRouting {
        expected_url: &'static str,
    }

    #[async_trait]
    impl Middleware for AssertRouting {
        async fn handle(&self, req: Request, next: Next<'_>) -> Result<Response> {
            assert_eq!(self.expected_url, &req.url().to_string());
            next.run(req).await
        }
    }

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
        let client = TwirpClientBuilder::new(base_url)
            .with(AssertRouting {
                expected_url: "http://localhost:3001/twirp/test.TestAPI/Ping",
            })
            .build()
            .unwrap();
        assert!(client
            .ping(PingRequest {
                name: "hi".to_string(),
            })
            .await
            .is_err()); // expected connection refused error.
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
}
