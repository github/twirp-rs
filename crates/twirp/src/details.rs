//! Undocumented features that are public for use in generated code (see `twirp-build`).

use std::future::Future;

use axum::extract::{Request, State};
use axum::Router;

use crate::{server, Context, TwirpErrorResponse};

/// Builder object used by generated code to build a Twirp service.
///
/// The type `S` is something like `Arc<MyExampleApiServer>`, which can be cheaply cloned for each
/// incoming request, providing access to the Rust value that actually implements the RPCs.
pub struct TwirpRouterBuilder<S> {
    service: S,
    router: Router<S>,
}

impl<S> TwirpRouterBuilder<S>
where
    S: Clone + Send + Sync + 'static,
{
    pub fn new(service: S) -> Self {
        TwirpRouterBuilder {
            service,
            router: Router::new(),
        }
    }

    /// Add a handler for an `rpc` to the router.
    ///
    /// The generated code passes a closure that calls the method, like
    /// `|api: Arc<HaberdasherApiServer>, req: MakeHatRequest| async move { api.make_hat(req) }`.
    pub fn route<F, Fut, Req, Res>(self, url: &str, f: F) -> Self
    where
        F: Fn(S, Context, Req) -> Fut + Clone + Sync + Send + 'static,
        Fut: Future<Output = Result<Res, TwirpErrorResponse>> + Send,
        Req: prost::Message + Default + serde::de::DeserializeOwned,
        Res: prost::Message + serde::Serialize,
    {
        TwirpRouterBuilder {
            service: self.service,
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
        self.router
            .fallback(crate::server::not_found_handler)
            .with_state(self.service)
    }
}
