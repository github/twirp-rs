use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use futures::Future;
use http_body_util::BodyExt;
use hyper::{header, Method, Request, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::time::{Duration, Instant};

use crate::headers::{CONTENT_TYPE_JSON, CONTENT_TYPE_PROTOBUF};
use crate::{error, Body, GenericError, TwirpErrorResponse};

/// A function that handles a request and returns a response.
type HandlerFn = Box<dyn Fn(Request<Body>) -> HandlerResponse + Send + Sync>;

/// Type alias for a handler response.
type HandlerResponse =
    Box<dyn Future<Output = Result<Response<Body>, GenericError>> + Unpin + Send>;

/// A Router maps a request (method, path) tuple to a handler.
pub struct Router {
    routes: HashMap<(Method, String), HandlerFn>,
    prefix: &'static str,
}

/// The canonical twirp path prefix. You don't have to use this, but it's the default.
pub const DEFAULT_TWIRP_PATH_PREFIX: &str = "/twirp";

impl Default for Router {
    fn default() -> Self {
        Self::new(DEFAULT_TWIRP_PATH_PREFIX)
    }
}

impl Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Router")
            .field("routes", &self.routes.keys())
            .finish()
    }
}

impl Router {
    /// Create a new router at the given prefix. Since this prefix is
    /// canonically `/twirp`, it is recommended to use `Router::default()`
    /// instead.
    pub fn new(prefix: &'static str) -> Self {
        Self {
            routes: Default::default(),
            prefix,
        }
    }

    /// Adds a sync handler to the router for the given method and path.
    pub fn add_sync_handler<F>(&mut self, method: Method, path: &str, f: F)
    where
        F: Fn(Request<Body>) -> Result<Response<Body>, GenericError>
            + Clone
            + Sync
            + Send
            + 'static,
    {
        let g = move |req| -> Box<
            dyn Future<Output = Result<Response<Body>, GenericError>> + Unpin + Send,
        > {
            let f = f.clone();
            Box::new(Box::pin(async move { f(req) }))
        };
        let key = (method, path.to_string());
        self.routes.insert(key, Box::new(g));
    }

    /// Adds an async handler to the router for the given method and path.
    pub fn add_handler<F, Fut>(&mut self, method: Method, path: &str, f: F)
    where
        F: Fn(Request<Body>) -> Fut + Clone + Sync + Send + 'static,
        Fut: Future<Output = Result<Response<Body>, GenericError>> + Send,
    {
        let g = move |req| -> Box<
            dyn Future<Output = Result<Response<Body>, GenericError>> + Unpin + Send,
        > {
            let f = f.clone();
            Box::new(Box::pin(async move { f(req).await }))
        };
        let key = (method, path.to_string());
        self.routes.insert(key, Box::new(g));
    }

    /// Adds a twirp method handler to the router for the given path.
    pub fn add_method<F, Fut, Req, Resp>(&mut self, path: &str, f: F)
    where
        F: Fn(Req) -> Fut + Clone + Sync + Send + 'static,
        Fut: Future<Output = Result<Resp, TwirpErrorResponse>> + Send,
        Req: prost::Message + Default + serde::de::DeserializeOwned,
        Resp: prost::Message + serde::Serialize,
    {
        let g = move |req: Request<Body>| -> Box<
            dyn Future<Output = Result<Response<Body>, GenericError>> + Unpin + Send,
        > {
            let f = f.clone();
            Box::new(Box::pin(async move {
                let mut timings = *req
                    .extensions()
                    .get::<Timings>()
                    .expect("invariant violated: timing info not present in request");
                match parse_request(req, &mut timings).await {
                    Ok((req, resp_fmt)) => {
                        let res = f(req).await;
                        timings.set_response_handled();
                        write_response(res, resp_fmt)
                    }
                    Err(err) => {
                        // This is the only place we use tracing (would be nice to remove)
                        // tracing::error!(?err, "failed to parse request");
                        // TODO: We don't want to loose the underlying error
                        // here, but it might not be safe to include in the
                        // response like this always.
                        let mut twirp_err = error::malformed("bad request");
                        twirp_err.insert_meta("error".to_string(), err.to_string());
                        twirp_err.to_response()
                    }
                }
                .map(|mut resp| {
                    timings.set_response_written();
                    resp.extensions_mut().insert(timings);
                    resp
                })
            }))
        };
        let key = (Method::POST, [self.prefix, path].join("/"));
        self.routes.insert(key, Box::new(g));
    }
}

/// Serve a request using the given router.
pub async fn serve(
    router: Arc<Router>,
    mut req: Request<Body>,
) -> Result<Response<Body>, GenericError> {
    if req.extensions().get::<Timings>().is_none() {
        let start = tokio::time::Instant::now();
        req.extensions_mut().insert(Timings::new(start));
    }
    let key = (req.method().clone(), req.uri().path().to_string());
    if let Some(handler) = router.routes.get(&key) {
        handler(req).await
    } else {
        error::bad_route("not found").to_response()
    }
}

// TODO: Properly implement JsonPb (de)serialization as it is slightly different
// than standard JSON.
#[derive(Debug, Clone, Copy, Default)]
enum BodyFormat {
    #[default]
    JsonPb,
    Pb,
}

impl BodyFormat {
    fn from_content_type(req: &Request<Body>) -> BodyFormat {
        match req
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|x| x.as_bytes())
        {
            Some(CONTENT_TYPE_PROTOBUF) => BodyFormat::Pb,
            _ => BodyFormat::JsonPb,
        }
    }
}

async fn parse_request<T>(
    req: Request<Body>,
    timings: &mut Timings,
) -> Result<(T, BodyFormat), GenericError>
where
    T: prost::Message + Default + DeserializeOwned,
{
    let format = BodyFormat::from_content_type(&req);
    let bytes = req.into_body().collect().await?.to_bytes();
    timings.set_received();
    let request = match format {
        BodyFormat::Pb => T::decode(bytes)?,
        BodyFormat::JsonPb => serde_json::from_slice(&bytes)?,
    };
    timings.set_parsed();
    Ok((request, format))
}

fn write_response<T>(
    response: Result<T, TwirpErrorResponse>,
    response_format: BodyFormat,
) -> Result<Response<Body>, GenericError>
where
    T: prost::Message + Serialize,
{
    let res = match response {
        Ok(response) => match response_format {
            BodyFormat::Pb => {
                let response = Response::builder()
                    .header(header::CONTENT_TYPE, CONTENT_TYPE_PROTOBUF)
                    .body(Body::from_proto_message(&response))?;
                Ok(response)
            }
            _ => {
                let data = serde_json::to_string(&response)?;
                let response = Response::builder()
                    .header(header::CONTENT_TYPE, CONTENT_TYPE_JSON)
                    .body(Body::from(data))?;
                Ok(response)
            }
        },
        Err(err) => err.to_response(),
    }?;
    Ok(res)
}

/// Contains timing information associated with a request.
/// To access the timings in a given request, use the [extensions](Request::extensions)
/// method and specialize to `Timings` appropriately.
#[derive(Debug, Clone, Copy)]
pub struct Timings {
    // When the request started.
    pub start: Instant,
    // When the request was received (headers and body).
    pub request_received: Option<Instant>,
    // When the request body was parsed.
    pub request_parsed: Option<Instant>,
    // When the response handler returned.
    pub response_handled: Option<Instant>,
    // When the response was written.
    pub response_written: Option<Instant>,
}

impl Timings {
    #[allow(clippy::new_without_default)]
    pub fn new(start: Instant) -> Self {
        Self {
            start,
            request_received: None,
            request_parsed: None,
            response_handled: None,
            response_written: None,
        }
    }

    fn set_received(&mut self) {
        self.request_received = Some(Instant::now());
    }

    fn set_parsed(&mut self) {
        self.request_parsed = Some(Instant::now());
    }

    fn set_response_handled(&mut self) {
        self.response_handled = Some(Instant::now());
    }

    fn set_response_written(&mut self) {
        self.response_written = Some(Instant::now());
    }

    pub fn received(&self) -> Option<Duration> {
        self.request_received.map(|x| x - self.start)
    }

    pub fn parsed(&self) -> Option<Duration> {
        match (self.request_parsed, self.request_received) {
            (Some(parsed), Some(received)) => Some(parsed - received),
            _ => None,
        }
    }

    pub fn response_handled(&self) -> Option<Duration> {
        match (self.response_handled, self.request_parsed) {
            (Some(handled), Some(parsed)) => Some(handled - parsed),
            _ => None,
        }
    }

    pub fn response_written(&self) -> Option<Duration> {
        match (self.response_written, self.response_handled) {
            (Some(written), Some(handled)) => Some(written - handled),
            (Some(written), None) => {
                if let Some(parsed) = self.request_parsed {
                    Some(written - parsed)
                } else {
                    self.request_received.map(|received| written - received)
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::test::*;

    #[tokio::test]
    async fn test_bad_route() {
        let router = Arc::new(Router::default());
        let req = Request::get("/nothing").body(Body::empty()).unwrap();
        let resp = serve(router, req).await.unwrap();
        let data = read_err_body(resp.into_body()).await;
        assert_eq!(data, error::bad_route("not found"));
    }

    #[tokio::test]
    async fn test_routes() {
        let router = test_api_router().await;
        assert!(router
            .routes
            .contains_key(&(Method::POST, "/twirp/test.TestAPI/Ping".to_string())));
        assert!(router
            .routes
            .contains_key(&(Method::POST, "/twirp/test.TestAPI/Boom".to_string())));
    }

    #[tokio::test]
    async fn test_ping_success() {
        let router = test_api_router().await;
        let resp = serve(router, gen_ping_request("hi")).await.unwrap();
        assert!(resp.status().is_success(), "{:?}", resp);
        let data: PingResponse = read_json_body(resp.into_body()).await;
        assert_eq!(&data.name, "hi");
    }

    #[tokio::test]
    async fn test_ping_invalid_request() {
        let router = test_api_router().await;
        let req = Request::post("/twirp/test.TestAPI/Ping")
            .body(Body::empty()) // not a valid request
            .unwrap();
        let resp = serve(router, req).await.unwrap();
        assert!(resp.status().is_client_error(), "{:?}", resp);
        let data = read_err_body(resp.into_body()).await;

        // TODO: I think malformed should return some info about what was wrong
        // with the request, but we don't want to leak server errors that have
        // other details.
        let mut expected = error::malformed("bad request");
        expected.insert_meta(
            "error".to_string(),
            "EOF while parsing a value at line 1 column 0".to_string(),
        );
        assert_eq!(data, expected);
    }

    #[tokio::test]
    async fn test_boom() {
        let router = test_api_router().await;
        let req = serde_json::to_string(&PingRequest {
            name: "hi".to_string(),
        })
        .unwrap();
        let req = Request::post("/twirp/test.TestAPI/Boom")
            .body(Body::from(req))
            .unwrap();
        let resp = serve(router, req).await.unwrap();
        assert!(resp.status().is_server_error(), "{:?}", resp);
        let data = read_err_body(resp.into_body()).await;
        assert_eq!(data, error::internal("boom!"));
    }
}
