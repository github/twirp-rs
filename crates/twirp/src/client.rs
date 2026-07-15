use std::collections::HashMap;
use std::sync::Arc;
use std::vec;

use async_trait::async_trait;
use http::header::Entry;
use http::header::IntoHeaderName;
use http::HeaderMap;
use http::HeaderValue;
use reqwest::header::CONTENT_TYPE;
use url::Host;
use url::Url;

use crate::headers::{CONTENT_TYPE_JSON, CONTENT_TYPE_PROTOBUF};
use crate::{serialize_proto_message, Result, TwirpErrorResponse};

/// Builder to easily create twirp clients.
pub struct ClientBuilder {
    base_url: Url,
    http_client: Option<reqwest::Client>,
    handlers: Option<RequestHandlers>,
    middleware: Vec<Box<dyn Middleware>>,
}

impl ClientBuilder {
    /// Creates a `twirp::ClientBuilder` with a base URL.
    pub fn new(base_url: Url) -> Self {
        Self {
            base_url,
            http_client: None,
            middleware: vec![],
            handlers: None,
        }
    }

    const DEFAULT_HOST: &'static str = "localhost";

    /// Creates a `twirp::ClientBuilder` suitable for registering request handlers instead of making http requests.
    /// NOTE: uses a default base URL and HTTP client.
    pub fn direct() -> Self {
        Self {
            base_url: Url::parse(&format!("http://{}/", Self::DEFAULT_HOST))
                .expect("must be a valid URL"),
            http_client: None,
            middleware: vec![],
            handlers: Some(RequestHandlers::new()),
        }
    }

    /// Set the HTTP client. Without this a default HTTP client is used.
    pub fn with_http_client(mut self, http_client: reqwest::Client) -> Self {
        self.http_client = Some(http_client);
        self
    }

    /// Add middleware to the client that will be called on each request.
    /// Middlewares are invoked in the order they are added as part of the
    /// request cycle.
    pub fn with_middleware<M>(mut self, middleware: M) -> Self
    where
        M: Middleware,
    {
        self.middleware.push(Box::new(middleware));
        self
    }

    /// Add a handler for a service using the default host.
    ///
    /// Warning: If you register `DirectHandler`s like this, they will be called instead of making HTTP requests.
    pub fn with_handler<M: DirectHandler + 'static>(self, handler: M) -> Self {
        self.with_handler_for_host(Self::DEFAULT_HOST, handler)
    }

    /// Add a handler for a service for a specific host.
    ///
    /// Warning: If you register `DirectHandler`s like this, they will be called instead of making HTTP requests.
    pub fn with_handler_for_host<M: DirectHandler + 'static>(
        mut self,
        host: &str,
        handler: M,
    ) -> Self {
        if let Some(handlers) = &mut self.handlers {
            handlers.add(host, handler);
        } else {
            panic!("you must use `ClientBuilder::direct()` to register handlers");
        }
        self
    }

    /// Set a default header for use in direct mode.
    pub fn with_default_header<K>(mut self, key: K, value: HeaderValue) -> Self
    where
        K: IntoHeaderName,
    {
        if let Some(handlers) = &mut self.handlers {
            handlers.default_headers.insert(key, value);
        } else {
            panic!("you must use `ClientBuilder::direct()` to register handler default headers");
        }
        self
    }

    /// Creates a `twirp::Client`.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn build(self) -> Client {
        let client = match self.handlers {
            Some(handlers) => ClientKind::Direct(handlers),
            None => ClientKind::Http(self.http_client.unwrap_or_default()),
        };
        Client::from_kind(self.base_url, client, self.middleware)
    }
}

/// `Client` is a Twirp HTTP client that uses `reqwest::Client` to make http
/// requests.
///
/// You do **not** have to wrap `Client` in an [`Rc`] or [`Arc`] to **reuse** it,
/// because it already uses an [`Arc`] internally.
#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientRef>,
    host: Option<String>,
}

// Contains references to data that is shared across all cloned copies of a client. The `Client` `host` field
// is deliberately not part of this data in order to support cloning a client and changing only the host.
struct ClientRef {
    base_url: Url,
    middlewares: Vec<Box<dyn Middleware>>,
    client: ClientKind,
}

enum ClientKind {
    Direct(RequestHandlers),
    Http(reqwest::Client),
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Client");
        debug.field("base_url", &self.inner.base_url);
        match &self.inner.client {
            ClientKind::Direct(_) => {
                debug.field("client", &"direct");
            }
            ClientKind::Http(client) => {
                debug.field("client", client);
            }
        }
        debug
            .field("middlewares", &self.inner.middlewares.len())
            .field(
                "handlers",
                &match &self.inner.client {
                    ClientKind::Direct(handlers) => handlers.len(),
                    ClientKind::Http(_) => 0,
                },
            )
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
        handlers: Option<RequestHandlers>,
    ) -> Self {
        let client = match handlers {
            Some(handlers) => ClientKind::Direct(handlers),
            None => ClientKind::Http(http_client),
        };
        Self::from_kind(base_url, client, middlewares)
    }

    fn from_kind(base_url: Url, client: ClientKind, middlewares: Vec<Box<dyn Middleware>>) -> Self {
        let base_url = if base_url.path().ends_with('/') {
            base_url
        } else {
            let mut base_url = base_url;
            let mut path = base_url.path().to_string();
            path.push('/');
            base_url.set_path(&path);
            base_url
        };
        Client {
            inner: Arc::new(ClientRef {
                base_url,
                middlewares,
                client,
            }),
            host: None,
        }
    }

    /// Creates a `twirp::Client` with the default `reqwest::ClientBuilder`.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn from_base_url(base_url: Url) -> Self {
        Self::new(base_url, reqwest::Client::new(), vec![], None)
    }

    /// The base URL of the service the client will call.
    pub fn base_url(&self) -> &Url {
        &self.inner.base_url
    }

    /// Creates a new `twirp::Client` with the same configuration as the current
    /// one, but with a different host in the base URL.
    pub fn with_host(&self, host: &str) -> Self {
        Self {
            inner: self.inner.clone(),
            host: Some(host.to_string()),
        }
    }

    /// Make an HTTP twirp request.
    pub async fn request<I, O>(
        &self,
        path: &str,
        req: http::Request<I>,
    ) -> Result<http::Response<O>>
    where
        I: prost::Message,
        O: prost::Message + Default,
    {
        let mut url = self.inner.base_url.join(path)?;
        if let Some(host) = &self.host {
            url.set_host(Some(host))?
        };
        let (parts, body) = req.into_parts();
        let body = serialize_proto_message(body);
        let request = match &self.inner.client {
            ClientKind::Direct(_) => {
                let mut request = reqwest::Request::new(reqwest::Method::POST, url);
                *request.headers_mut() = parts.headers;
                request.headers_mut().append(
                    CONTENT_TYPE,
                    HeaderValue::from_bytes(CONTENT_TYPE_PROTOBUF)
                        .expect("protobuf content type must be valid"),
                );
                *request.body_mut() = Some(body.into());
                request
            }
            ClientKind::Http(client) => client
                .post(url)
                .headers(parts.headers)
                .header(CONTENT_TYPE, CONTENT_TYPE_PROTOBUF)
                .body(body)
                .build()?,
        };

        // Create and execute the middleware handlers
        let next = Next::new(&self.inner.client, &self.inner.middlewares);
        let response = next.run(request).await?;

        // These have to be extracted because reading the body consumes `Response`.
        let version = response.version();
        let status = response.status();
        let headers = response.headers().clone();
        let extensions = response.extensions().clone();
        let content_type = headers.get(CONTENT_TYPE).cloned();

        // TODO: Include more info in the error cases: request path, content-type, etc.
        match (status, content_type) {
            (status, Some(ct)) if status.is_success() && ct.as_bytes() == CONTENT_TYPE_PROTOBUF => {
                O::decode(response.bytes().await?)
                    .map(|x| {
                        let mut resp = http::Response::new(x);
                        *resp.version_mut() = version;
                        resp.headers_mut().extend(headers);
                        resp.extensions_mut().extend(extensions);
                        resp
                    })
                    .map_err(|e| e.into())
            }
            (status, Some(ct))
                if (status.is_client_error() || status.is_server_error())
                    && ct.as_bytes() == CONTENT_TYPE_JSON =>
            {
                // TODO: Should middleware response extensions and headers be included in the error case?
                Err(serde_json::from_slice(&response.bytes().await?)?)
            }
            (status, ct) => Err(TwirpErrorResponse::new(
                status.into(),
                format!("unexpected content type: {:?}", ct),
            )),
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
    client: &'a ClientKind,
    middlewares: &'a [Box<dyn Middleware>],
}

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

impl<'a> Next<'a> {
    fn new(client: &'a ClientKind, middlewares: &'a [Box<dyn Middleware>]) -> Self {
        Next {
            client,
            middlewares,
        }
    }

    pub fn run(mut self, req: reqwest::Request) -> BoxFuture<'a, Result<reqwest::Response>> {
        if let Some((current, rest)) = self.middlewares.split_first() {
            // Run any middleware
            self.middlewares = rest;
            Box::pin(current.handle(req, self))
        } else {
            match self.client {
                ClientKind::Direct(handlers) => {
                    Box::pin(async move { execute_handlers(req, handlers).await })
                }
                ClientKind::Http(client) => Box::pin(async move { Ok(client.execute(req).await?) }),
            }
        }
    }
}

async fn execute_handlers(
    mut req: reqwest::Request,
    request_handlers: &RequestHandlers,
) -> Result<reqwest::Response> {
    let req_headers = req.headers_mut();
    for (key, value) in &request_handlers.default_headers {
        if let Entry::Vacant(entry) = req_headers.entry(key) {
            entry.insert(value.clone());
        }
    }
    let url = req.url().clone();
    let Some(mut segments) = url.path_segments() else {
        return Err(crate::bad_route(format!(
            "invalid request to {}: no path segments",
            url
        )));
    };
    let (Some(method), Some(service)) = (segments.next_back(), segments.next_back()) else {
        return Err(crate::bad_route(format!(
            "invalid request to {}: method and service required",
            url
        )));
    };
    let host = url.host().expect("no host in url");

    if let Some(handler) = request_handlers.get(&host, service) {
        handler.handle(method, req).await
    } else {
        Err(crate::bad_route(format!(
            "no handler found for host: '{host}', service: '{service}'"
        )))
    }
}

#[derive(Clone, Default)]
pub struct RequestHandlers {
    default_headers: HeaderMap,
    /// A map of host/service names to handlers.
    handlers: HashMap<String, Arc<dyn DirectHandler>>,
}

impl RequestHandlers {
    pub fn new() -> Self {
        Self {
            default_headers: HeaderMap::new(),
            handlers: HashMap::new(),
        }
    }

    pub fn add<M: DirectHandler + 'static>(&mut self, host: &str, handler: M) {
        let key = format!("{}/{}", host, handler.service());
        self.handlers.insert(key, Arc::new(handler));
    }

    pub fn get(&self, host: &Host<&str>, service: &str) -> Option<Arc<dyn DirectHandler>> {
        self.handlers.get(&format!("{}/{}", host, service)).cloned()
    }

    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

#[async_trait]
pub trait DirectHandler: 'static + Send + Sync {
    fn service(&self) -> &str;
    async fn handle(&self, path: &str, mut req: reqwest::Request) -> Result<reqwest::Response>;
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
        assert_eq!(
            Client::from_base_url(url).base_url().to_string(),
            "http://localhost:3001/twirp/"
        );
        let url = Url::parse("http://localhost:3001/twirp").unwrap();
        assert_eq!(
            Client::from_base_url(url).base_url().to_string(),
            "http://localhost:3001/twirp/"
        );
    }

    #[test]
    fn test_client_builder_modes() {
        let direct = ClientBuilder::direct().build();
        assert!(matches!(direct.inner.client, ClientKind::Direct(_)));

        let base_url = Url::parse("http://localhost:3001/twirp/").unwrap();
        let http = ClientBuilder::new(base_url).build();
        assert!(matches!(http.inner.client, ClientKind::Http(_)));
    }

    #[tokio::test]
    async fn test_routes() {
        let base_url = Url::parse("http://localhost:3001/twirp/").unwrap();

        let client = ClientBuilder::new(base_url)
            .with_middleware(AssertRouting {
                expected_url: "http://localhost:3001/twirp/test.TestAPI/Ping",
            })
            .build();
        assert!(client
            .ping(http::Request::new(PingRequest {
                name: "hi".to_string(),
            }))
            .await
            .is_err()); // expected connection refused error.
    }

    #[tokio::test]
    async fn test_standard_client() {
        let h = run_test_server(3002).await;
        let base_url = Url::parse("http://localhost:3002/twirp/").unwrap();
        let client = Client::from_base_url(base_url);
        let resp = client
            .ping(http::Request::new(PingRequest {
                name: "hi".to_string(),
            }))
            .await
            .unwrap();
        let data = resp.into_body();
        assert_eq!(data.name, "hi");
        h.abort()
    }
}
