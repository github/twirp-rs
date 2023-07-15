use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use futures::Future;
use hyper::{header, Body, Method, Request, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::*;
use crate::headers::*;
use crate::to_proto_body;

type HandlerResponse =
    Box<dyn Future<Output = Result<Response<Body>, GenericError>> + Unpin + Send>;

type HandlerFn = Box<dyn Fn(Request<Body>) -> HandlerResponse + Send + Sync>;

/// A Router maps a request to a handler.
pub struct Router {
    routes: HashMap<(Method, String), HandlerFn>,
    prefix: &'static str,
}

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

    /// Adds a handler to the router for the given method and path.
    pub fn add_handler<F>(&mut self, method: Method, path: &str, f: F)
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
                match parse_request(req).await {
                    Ok((req, resp_fmt)) => write_response(f(req).await, resp_fmt),
                    Err(err) => {
                        // This is the only place we use tracing (would be nice to remove)
                        // tracing::error!(?err, "failed to parse request");
                        // TODO: We don't want to loose the underlying error
                        // here, but it might not be safe to include in the
                        // response like this always.
                        let mut twirp_err = malformed("bad request");
                        twirp_err.insert_meta("error".to_string(), err.to_string());
                        twirp_err.to_response()
                    }
                }
            }))
        };
        let key = (Method::POST, [self.prefix, path].join("/"));
        self.routes.insert(key, Box::new(g));
    }
}

pub async fn serve(
    router: Arc<Router>,
    req: Request<Body>,
) -> Result<Response<Body>, GenericError> {
    let key = (req.method().clone(), req.uri().path().to_string());
    if let Some(handler) = router.routes.get(&key) {
        handler(req).await
    } else {
        bad_route("not found").to_response()
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

async fn parse_request<T>(req: Request<Body>) -> Result<(T, BodyFormat), GenericError>
where
    T: prost::Message + Default + DeserializeOwned,
{
    let format = BodyFormat::from_content_type(&req);
    let bytes = hyper::body::to_bytes(req.into_body()).await?;
    let request = match format {
        BodyFormat::Pb => T::decode(bytes)?,
        BodyFormat::JsonPb => serde_json::from_slice(&bytes)?,
    };
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
                    .body(to_proto_body(response))?;
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
        assert_eq!(data, bad_route("not found"));
    }

    #[tokio::test]
    async fn test_routes() {
        let router = test_api_router().await;
        let (method, path) = router.routes.iter().next().unwrap().0;
        assert_eq!(method, Method::POST);
        assert_eq!(path, "/twirp/test.TestAPI/Ping");
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
        let mut expected = malformed("bad request");
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
        assert_eq!(data, internal("boom!"));
    }
}
