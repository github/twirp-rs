use std::sync::Arc;
use std::vec;

use async_trait::async_trait;
use http::{HeaderName, HeaderValue};
use reqwest::header::{InvalidHeaderValue, CONTENT_TYPE};
use reqwest::StatusCode;
use thiserror::Error;
use url::Url;

use crate::headers::{CONTENT_TYPE_JSON, CONTENT_TYPE_PROTOBUF};
use crate::{serialize_proto_message, GenericError, TwirpErrorResponse};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ClientError {
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

    /// A generic error that can be used by custom middleware.
    #[error(transparent)]
    MiddlewareError(#[from] GenericError),
}

pub type Result<T, E = ClientError> = std::result::Result<T, E>;

pub struct ClientBuilder {
    base_url: Url,
    http_client: reqwest::Client,
    middleware: Vec<Box<dyn Middleware>>,
}

impl ClientBuilder {
    pub fn new(base_url: Url, http_client: reqwest::Client) -> Self {
        Self {
            base_url,
            middleware: vec![],
            http_client,
        }
    }

    /// Add middleware to the client that will be called on each request.
    /// Middlewares are invoked in the order they are added as part of the
    /// request cycle.
    pub fn with<M>(self, middleware: M) -> Self
    where
        M: Middleware,
    {
        let mut mw = self.middleware;
        mw.push(Box::new(middleware));
        Self {
            base_url: self.base_url,
            http_client: self.http_client,
            middleware: mw,
        }
    }

    pub fn build(self) -> Result<Client> {
        Client::new(self.base_url, self.http_client, self.middleware)
    }
}

/// `Client` is a Twirp HTTP client that uses `reqwest::Client` to make http
/// requests.
///
/// You do **not** have to wrap `Client` in an [`Rc`] or [`Arc`] to **reuse** it,
/// because it already uses an [`Arc`] internally.
#[derive(Clone)]
pub struct Client {
    http_client: reqwest::Client,
    inner: Arc<ClientRef>,
    host: Option<String>,
}

struct ClientRef {
    base_url: Url,
    middlewares: Vec<Box<dyn Middleware>>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.inner.base_url)
            .field("client", &self.http_client)
            .field("middlewares", &self.inner.middlewares.len())
            .finish()
    }
}

impl Client {
    /// Creates a `twirp::Client`.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn new(
        base_url: Url,
        http_client: reqwest::Client,
        middlewares: Vec<Box<dyn Middleware>>,
    ) -> Result<Self> {
        if base_url.path().ends_with('/') {
            Ok(Client {
                http_client,
                inner: Arc::new(ClientRef {
                    base_url,
                    middlewares,
                }),
                host: None,
            })
        } else {
            Err(ClientError::InvalidBaseUrl(base_url))
        }
    }

    /// Creates a `twirp::Client` with the default `reqwest::ClientBuilder`.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn from_base_url(base_url: Url) -> Result<Self> {
        Self::new(base_url, reqwest::Client::new(), vec![])
    }

    pub fn base_url(&self) -> &Url {
        &self.inner.base_url
    }

    /// Creates a new `twirp::Client` with the same configuration as the current
    /// one, but with a different host in the base URL.
    pub fn with_host(&self, host: &str) -> Self {
        Self {
            http_client: self.http_client.clone(),
            inner: self.inner.clone(),
            host: Some(host.to_string()),
        }
    }

    /// Executes a `Request`.
    pub(super) async fn execute<O>(&self, req: reqwest::Request) -> Result<O>
    where
        O: prost::Message + Default,
    {
        let path = req.url().path().to_string();

        // Create and execute the middleware handlers
        let next = Next::new(&self.http_client, &self.inner.middlewares);
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
                Err(ClientError::TwirpError(serde_json::from_slice(
                    &resp.bytes().await?,
                )?))
            }
            (status, ct) => Err(ClientError::HttpError {
                status,
                msg: "unknown error".to_string(),
                path,
                content_type: ct
                    .map(|x| x.to_str().unwrap_or_default().to_string())
                    .unwrap_or_default(),
            }),
        }
    }

    // Start building a request...
    pub fn request<I, O>(&self, path: &str, body: I) -> Result<RequestBuilder<I, O>>
    where
        I: prost::Message,
        O: prost::Message + Default,
    {
        let mut url = self.inner.base_url.join(path)?;
        if let Some(host) = &self.host {
            url.set_host(Some(host))?
        };

        let req = self
            .http_client
            .post(url)
            .header(CONTENT_TYPE, CONTENT_TYPE_PROTOBUF)
            .body(serialize_proto_message(body));
        Ok(RequestBuilder::new(self.clone(), req))
    }
}

pub struct RequestBuilder<I, O>
where
    O: prost::Message + Default,
{
    client: Client,
    inner: reqwest::RequestBuilder,
    _input: std::marker::PhantomData<I>,
    _output: std::marker::PhantomData<O>,
}

impl<I, O> RequestBuilder<I, O>
where
    O: prost::Message + Default,
{
    pub fn new(client: Client, inner: reqwest::RequestBuilder) -> Self {
        Self {
            client,
            inner,
            _input: std::marker::PhantomData,
            _output: std::marker::PhantomData,
        }
    }

    /// Add a `Header` to this Request.
    pub fn header<K, V>(mut self, key: K, value: V) -> RequestBuilder<I, O>
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.inner = self.inner.header(key, value);
        self
    }

    pub async fn send(self) -> Result<O> {
        let req = self.inner.build()?;
        self.client.execute(req).await
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
    middlewares: &'a [Box<dyn Middleware>],
}

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

impl<'a> Next<'a> {
    pub(crate) fn new(client: &'a reqwest::Client, middlewares: &'a [Box<dyn Middleware>]) -> Self {
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
            Box::pin(async move { self.client.execute(req).await.map_err(ClientError::from) })
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
        assert!(Client::from_base_url(url).is_ok());
        let url = Url::parse("http://localhost:3001/twirp").unwrap();
        assert_eq!(
            Client::from_base_url(url).unwrap_err().to_string(),
            "base_url must end in /, but got: http://localhost:3001/twirp",
        );
    }

    #[tokio::test]
    async fn test_routes() {
        let base_url = Url::parse("http://localhost:3001/twirp/").unwrap();

        let client = ClientBuilder::new(base_url, reqwest::Client::new())
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
    async fn test_standard_client() {
        let h = run_test_server(3002).await;
        let base_url = Url::parse("http://localhost:3002/twirp/").unwrap();
        let client = Client::from_base_url(base_url).unwrap();
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
