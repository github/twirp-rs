//! Test helpers and mini twirp api server implementation.
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::Router;
use http_body_util::BodyExt;
use hyper::Request;
use serde::de::DeserializeOwned;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::details::TwirpRouterBuilder;
use crate::server::Timings;
use crate::{error, Client, Result, TwirpErrorResponse};

pub async fn run_test_server(port: u16) -> JoinHandle<Result<(), std::io::Error>> {
    let router = test_api_router();
    let addr: std::net::SocketAddr = ([127, 0, 0, 1], port).into();
    let tcp_listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind to local port");
    println!("Listening on {addr}");
    let h = tokio::spawn(async move { axum::serve(tcp_listener, router).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    h
}

pub fn test_api_router() -> Router {
    let api = Arc::new(TestApiServer {});

    // NB: This part would be generated
    let test_router = TwirpRouterBuilder::new(api)
        .route(
            "/Ping",
            |api: Arc<TestApiServer>, req: http::Request<PingRequest>| async move {
                api.ping(req).await
            },
        )
        .route(
            "/Boom",
            |api: Arc<TestApiServer>, req: http::Request<PingRequest>| async move {
                api.boom(req).await
            },
        )
        .build();

    axum::Router::new()
        .nest("/twirp/test.TestAPI", test_router)
        .fallback(crate::server::not_found_handler)
}

pub fn gen_ping_request(name: &str) -> Request<Body> {
    let req = serde_json::to_string(&PingRequest {
        name: name.to_string(),
    })
    .expect("will always be valid json");
    Request::post("/twirp/test.TestAPI/Ping")
        .extension(Timings::new(Instant::now()))
        .body(Body::from(req))
        .expect("always a valid twirp request")
}

pub async fn read_string_body(body: Body) -> String {
    let data = Vec::<u8>::from(body.collect().await.expect("invalid body").to_bytes());
    String::from_utf8(data).expect("non-utf8 body")
}

pub async fn read_json_body<T>(body: Body) -> T
where
    T: DeserializeOwned,
{
    let data = Vec::<u8>::from(body.collect().await.expect("invalid body").to_bytes());
    serde_json::from_slice(&data).expect("twirp response isn't valid JSON")
}

pub async fn read_err_body(body: Body) -> TwirpErrorResponse {
    read_json_body(body).await
}

// Hand written sample test server and client

pub struct TestApiServer;

#[async_trait]
impl TestApi for TestApiServer {
    async fn ping(
        &self,
        req: http::Request<PingRequest>,
    ) -> Result<http::Response<PingResponse>, TwirpErrorResponse> {
        let request_id = req.extensions().get::<RequestId>().cloned();
        let data = req.into_body();
        if let Some(RequestId(rid)) = request_id {
            Ok(http::Response::new(PingResponse {
                name: format!("{}-{}", data.name, rid),
            }))
        } else {
            Ok(http::Response::new(PingResponse { name: data.name }))
        }
    }

    async fn boom(
        &self,
        _: http::Request<PingRequest>,
    ) -> Result<http::Response<PingResponse>, TwirpErrorResponse> {
        Err(error::internal("boom!"))
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, Default)]
pub struct RequestId(pub String);

// Small test twirp services (this would usually be generated with twirp-build)
#[async_trait]
pub trait TestApiClient {
    async fn ping(&self, req: http::Request<PingRequest>) -> Result<http::Response<PingResponse>>;
    async fn boom(&self, req: http::Request<PingRequest>) -> Result<http::Response<PingResponse>>;
}

#[async_trait]
impl TestApiClient for Client {
    async fn ping(&self, req: http::Request<PingRequest>) -> Result<http::Response<PingResponse>> {
        self.request("test.TestAPI/Ping", req).await
    }

    async fn boom(&self, _req: http::Request<PingRequest>) -> Result<http::Response<PingResponse>> {
        todo!()
    }
}

#[async_trait]
pub trait TestApi {
    async fn ping(
        &self,
        req: http::Request<PingRequest>,
    ) -> Result<http::Response<PingResponse>, TwirpErrorResponse>;
    async fn boom(
        &self,
        req: http::Request<PingRequest>,
    ) -> Result<http::Response<PingResponse>, TwirpErrorResponse>;
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PingRequest {
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PingResponse {
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
