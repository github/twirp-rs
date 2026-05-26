//! Undocumented features that are public for use in generated code (see `twirp-build`).

use std::future::Future;

use axum::extract::{Request, State};
use axum::Router;
use http_body_util::BodyExt;

use crate::{malformed, serialize_proto_message, server, Result, TwirpErrorResponse};

/// Builder object used by generated code to build a Twirp service.
///
/// The type `S` is something like `Arc<MyExampleApiServer>`, which can be cheaply cloned for each
/// incoming request, providing access to the Rust value that actually implements the RPCs.
pub struct TwirpRouterBuilder<S> {
    service: S,
    fqn: &'static str,
    router: Router<S>,
}

impl<S> TwirpRouterBuilder<S>
where
    S: Clone + Send + Sync + 'static,
{
    pub fn new(fqn: &'static str, service: S) -> Self {
        TwirpRouterBuilder {
            service,
            fqn,
            router: Router::new(),
        }
    }

    /// Add a handler for an `rpc` to the router.
    ///
    /// The generated code passes a closure that calls the method, like
    /// `|api: Arc<HaberdasherApiServer>, req: http::Request<MakeHatRequest>| async move { api.make_hat(req) }`.
    pub fn route<F, Fut, Req, Res>(self, url: &str, f: F) -> Self
    where
        F: Fn(S, http::Request<Req>) -> Fut + Clone + Sync + Send + 'static,
        Fut: Future<Output = Result<http::Response<Res>, TwirpErrorResponse>> + Send,
        Req: prost::Message + Default + serde::de::DeserializeOwned,
        Res: prost::Message + Default + serde::Serialize,
    {
        TwirpRouterBuilder {
            service: self.service,
            fqn: self.fqn,
            router: self.router.route(
                url,
                axum::routing::post(move |State(api): State<S>, req: Request| async move {
                    server::handle_request(api, req, f).await
                }),
            ),
        }
    }

    /// Finish building the axum router.
    pub fn build(self) -> axum::Router {
        Router::new().nest(
            self.fqn,
            self.router
                .fallback(crate::server::not_found_handler)
                .with_state(self.service),
        )
    }
}

/// Decode a `reqwest::Request` into a `http::Request<I>`.
pub async fn decode_request<I>(req: reqwest::Request) -> Result<http::Request<I>>
where
    I: prost::Message + Default,
{
    let http_req: http::Request<reqwest::Body> = req
        .try_into()
        .map_err(|e| malformed(format!("failed to convert request: {e}")))?;
    let (parts, body) = http_req.into_parts();
    let bytes = body
        .collect()
        .await
        .map_err(|e| malformed(format!("failed to read the request body: {e}")))?
        .to_bytes();
    let data = I::decode(bytes).map_err(|e| malformed(format!("failed to decode request: {e}")))?;
    let mut req = http::Request::new(data);
    *req.method_mut() = parts.method;
    *req.uri_mut() = parts.uri;
    *req.version_mut() = parts.version;
    *req.headers_mut() = parts.headers;
    *req.extensions_mut() = parts.extensions;
    Ok(req)
}

/// Encode a `http::Response<O>` into a `reqwest::Response`.
pub fn encode_response<O>(resp: http::Response<O>) -> Result<reqwest::Response>
where
    O: prost::Message + Default,
{
    let mut resp = resp.map(serialize_proto_message);
    resp.headers_mut()
        .insert("Content-Type", "application/protobuf".try_into()?);
    Ok(reqwest::Response::from(resp))
}
