use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use futures::Future;
use hyper::{header, Body, Method, Request, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub mod error;
pub use error::*;

type HandlerResponse =
    Box<dyn Future<Output = Result<Response<Body>, GenericError>> + Unpin + Send>;

type HandlerFn = Box<dyn Fn(Request<Body>) -> HandlerResponse + Send + Sync>;

#[derive(Default)]
pub struct Router {
    routes: HashMap<(Method, String), HandlerFn>,
}

impl Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Router")
            .field("routes", &self.routes.keys())
            .finish()
    }
}

impl Router {
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
                        tracing::error!(?err, "failed to parse request");
                        malformed("bad request").to_response()
                    }
                }
            }))
        };
        let key = (Method::POST, path.to_string());
        self.routes.insert(key, Box::new(g));
    }
}

pub async fn serve(table: Arc<Router>, req: Request<Body>) -> Result<Response<Body>, GenericError> {
    let key = (req.method().clone(), req.uri().path().to_string());
    if let Some(handler) = table.routes.get(&key) {
        handler(req).await.or_else(|err| {
            tracing::error!(?err, path=?key.1, "internal server error");
            internal("internal server error").to_response()
        })
    } else {
        tracing::error!(path=?key.1, "no handler registered for path");
        bad_route("not found").to_response()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum BodyFormat {
    #[default]
    JsonPb,
    Pb,
}

impl BodyFormat {
    pub fn from_content_type(req: &Request<Body>) -> BodyFormat {
        match req
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|x| x.as_bytes())
        {
            Some(b"application/protobuf") => BodyFormat::Pb,
            _ => BodyFormat::JsonPb,
        }
    }
}

async fn parse_request<T>(req: Request<Body>) -> Result<(T, BodyFormat), GenericError>
where
    T: prost::Message + Default + DeserializeOwned,
{
    let request_format = BodyFormat::from_content_type(&req);

    let response_format = match req.headers().get(header::ACCEPT).map(|x| x.as_bytes()) {
        Some(b"application/protobuf") => BodyFormat::Pb,
        _ => BodyFormat::JsonPb,
    };

    let bytes = hyper::body::to_bytes(req.into_body()).await?;
    let request = match request_format {
        BodyFormat::Pb => T::decode(bytes)?,
        BodyFormat::JsonPb => serde_json::from_slice(&bytes)?,
    };
    Ok((request, response_format))
}

pub fn write_response<T>(
    response: Result<T, TwirpErrorResponse>,
    response_format: BodyFormat,
) -> Result<Response<Body>, GenericError>
where
    T: prost::Message + Serialize,
{
    let res = match response {
        Ok(response) => match response_format {
            BodyFormat::Pb => {
                let len = response.encoded_len();
                let mut data = Vec::with_capacity(len);
                response
                    .encode(&mut data)
                    .expect("can only fail if buffer does not have capacity");
                assert_eq!(data.len(), len);
                let response = Response::builder()
                    .header(header::CONTENT_TYPE, "application/protobuf")
                    .body(Body::from(data))?;
                Ok(response)
            }
            _ => {
                let data = serde_json::to_string(&response)?;
                let response = Response::builder()
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(data))?;
                Ok(response)
            }
        },
        Err(err) => err.to_response(),
    }?;
    Ok(res)
}

pub fn write_err_response(err: TwirpErrorResponse) -> Result<Response<Body>, GenericError> {
    err.to_response()
}
