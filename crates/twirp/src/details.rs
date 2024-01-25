//! Undocumented features that are public for use in generated code (see `twirp-build`).

use std::future::Future;

use axum::extract::{Request, State};
use axum::Router;

use crate::{server, TwirpErrorResponse};

/// Builder object used by generated code to build a router.
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

    pub fn route<F, G, Fut, RequestMessage, ResponseMessage>(self, url: &str, f: F) -> Self
    where
        F: Fn(S) -> G + Clone + Send + 'static,
        G: FnOnce(RequestMessage) -> Fut + Clone + Sync + Send + 'static,
        Fut: Future<Output = Result<ResponseMessage, TwirpErrorResponse>> + Send,
        RequestMessage: prost::Message + Default + serde::de::DeserializeOwned,
        ResponseMessage: prost::Message + serde::Serialize,
    {
        TwirpRouterBuilder {
            service: self.service,
            router: self.router.route(
                url,
                axum::routing::post(move |State(api): State<S>, req: Request| async move {
                    server::handle_request(req, f(api)).await
                }),
            ),
        }
    }

    pub fn build(self) -> axum::Router {
        self.router
            .fallback(crate::server::not_found_handler)
            .with_state(self.service)
    }
}
