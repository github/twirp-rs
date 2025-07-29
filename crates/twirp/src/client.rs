use std::collections::HashMap;
use std::sync::Arc;
use std::vec;

use async_trait::async_trait;
use reqwest::header::CONTENT_TYPE;
use url::Host;
use url::Url;

use crate::headers::{CONTENT_TYPE_JSON, CONTENT_TYPE_PROTOBUF};
use crate::{serialize_proto_message, Result, TwirpErrorResponse};

/// Builder to easily create twirp clients.
pub struct ClientBuilder {
    base_url: Url,
    http_client: reqwest::Client,
    handlers: RequestHandlers,
    middleware: Vec<Box<dyn Middleware>>,
}

impl ClientBuilder {
    /// Creates a `twirp::ClientBuilder` with a base URL and HTTP client.
    pub fn new(base_url: Url, http_client: reqwest::Client) -> Self {
        Self {
            base_url,
            http_client,
            middleware: vec![],
            handlers: RequestHandlers::new(),
        }
    }

    const DEFAULT_HOST: &'static str = "localhost";

    /// Creates a `twirp::ClientBuilder` suitable for registering request handlers instead of making http requests.
    /// NOTE: uses a default base URL and HTTP client.
    pub fn direct() -> Self {
        Self {
            base_url: Url::parse(&format!("http://{}/", Self::DEFAULT_HOST))
                .expect("must be a valid URL"),
            http_client: reqwest::Client::new(),
            middleware: vec![],
            handlers: RequestHandlers::new(),
        }
    }

    /// Add middleware to the client that will be called on each request.
    /// Middlewares are invoked in the order they are added as part of the
    /// request cycle.
    pub fn with_middleware<M>(self, middleware: M) -> Self
    where
        M: Middleware,
    {
        let mut mw = self.middleware;
        mw.push(Box::new(middleware));
        Self {
            base_url: self.base_url,
            http_client: self.http_client,
            handlers: self.handlers,
            middleware: mw,
        }
    }

    /// Add a handler for a service using the default host.
    pub fn with_handler<M: DirectHandler + 'static>(self, handler: M) -> Self {
        self.with_handler_for_host(Self::DEFAULT_HOST, handler)
    }

    /// Add a handler for a service for a specific host.
    pub fn with_handler_for_host<M: DirectHandler + 'static>(
        mut self,
        host: &str,
        handler: M,
    ) -> Self {
        self.handlers.add(host, handler);
        self
    }

    /// Creates a `twirp::Client`.
    ///
    /// The underlying `reqwest::Client` holds a connection pool internally, so it is advised that
    /// you create one and **reuse** it.
    pub fn build(self) -> Client {
        let handlers = if self.handlers.handlers.is_empty() {
            None
        } else {
            Some(self.handlers)
        };
        Client::new(self.base_url, self.http_client, self.middleware, handlers)
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
    handlers: Option<RequestHandlers>,
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
        handlers: Option<RequestHandlers>,
    ) -> Self {
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
            http_client,
            inner: Arc::new(ClientRef {
                base_url,
                middlewares,
            }),
            host: None,
            handlers,
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
            http_client: self.http_client.clone(),
            inner: self.inner.clone(),
            host: Some(host.to_string()),
            handlers: self.handlers.clone(),
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
        let request = self
            .http_client
            .post(url)
            .headers(parts.headers)
            .header(CONTENT_TYPE, CONTENT_TYPE_PROTOBUF)
            .body(serialize_proto_message(body))
            .build()?;

        // Create and execute the middleware handlers
        let next = Next::new(
            &self.http_client,
            &self.inner.middlewares,
            self.handlers.as_ref(),
        );
        let response = next.run(request).await?;

        // These have to be extracted because reading the body consumes `Response`.
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
    client: &'a reqwest::Client,
    middlewares: &'a [Box<dyn Middleware>],
    handlers: Option<&'a RequestHandlers>,
}

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

impl<'a> Next<'a> {
    pub(crate) fn new(
        client: &'a reqwest::Client,
        middlewares: &'a [Box<dyn Middleware>],
        handlers: Option<&'a RequestHandlers>,
    ) -> Self {
        Next {
            client,
            middlewares,
            handlers,
        }
    }

    pub fn run(mut self, req: reqwest::Request) -> BoxFuture<'a, Result<reqwest::Response>> {
        if let Some((current, rest)) = self.middlewares.split_first() {
            // Run any middleware
            self.middlewares = rest;
            Box::pin(current.handle(req, self))
        } else if let Some(handlers) = self.handlers {
            // If we've got a client with direct request handlers: use those
            Box::pin(async move { execute_handlers(req, handlers).await })
        } else {
            // Otherwise: execute the actual http request here
            Box::pin(async move { Ok(self.client.execute(req).await?) })
        }
    }
}

async fn execute_handlers(
    req: reqwest::Request,
    request_handlers: &RequestHandlers,
) -> Result<reqwest::Response> {
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
    /// A map of host/service names to handlers.
    handlers: HashMap<String, Arc<dyn DirectHandler>>,
}

impl RequestHandlers {
    pub fn new() -> Self {
        Self {
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

    #[tokio::test]
    async fn test_routes() {
        let base_url = Url::parse("http://localhost:3001/twirp/").unwrap();

        let client = ClientBuilder::new(base_url, reqwest::Client::new())
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
