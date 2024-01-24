//! Test helpers and mini twirp api server implementation.
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde::de::DeserializeOwned;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::{error, Body, Client, Result, Router, TwirpErrorResponse};

async fn test_server_handle_connection(io: TokioIo<TcpStream>, router: Arc<Router>) {
    let service = service_fn(move |req: Request<Incoming>| {
        let router = Arc::clone(&router);
        async move {
            let req = req.map(|body| Body::new(body));
            crate::serve(router.clone(), req).await
        }
    });
    if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
        eprintln!("Error serving connection: {err:?}");
    }
}

async fn test_server_main(
    tcp_listener: TcpListener,
    router: Arc<Router>,
) -> Result<(), std::io::Error> {
    loop {
        let (stream, _) = tcp_listener.accept().await?;
        let io = TokioIo::new(stream);
        let router = Arc::clone(&router);
        tokio::spawn(test_server_handle_connection(io, router));
    }
}

pub async fn run_test_server(port: u16) -> JoinHandle<Result<(), std::io::Error>> {
    let router = test_api_router().await;
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let tcp_listener = TcpListener::bind(&addr).await.unwrap();
    let server = test_server_main(tcp_listener, router);
    println!("Listening on {addr}");
    let h = tokio::spawn(server);
    tokio::time::sleep(Duration::from_millis(100)).await;
    h
}

pub async fn test_api_router() -> Arc<Router> {
    let api = Arc::new(TestAPIServer {});
    let mut router = Router::default();
    // NB: This would be generated
    {
        let api = api.clone();
        router.add_method("test.TestAPI/Ping", move |req| {
            let api = api.clone();
            async move { api.ping(req).await }
        });
    }
    {
        router.add_method("test.TestAPI/Boom", move |req| {
            let api = api.clone();
            async move { api.boom(req).await }
        });
    }
    Arc::new(router)
}

pub fn gen_ping_request(name: &str) -> Request<Body> {
    let req = serde_json::to_string(&PingRequest {
        name: name.to_string(),
    })
    .expect("will always be valid json");
    Request::post("/twirp/test.TestAPI/Ping")
        .body(Body::from(req))
        .expect("always a valid twirp request")
}

pub async fn read_string_body(body: Body) -> String {
    let data = body
        .collect()
        .await
        .expect("invalid body")
        .to_bytes()
        .to_vec();
    String::from_utf8(data).expect("non-utf8 body")
}

pub async fn read_json_body<T>(body: Body) -> T
where
    T: DeserializeOwned,
{
    let data = body.collect().await.expect("invalid body").to_bytes();
    serde_json::from_slice(&data).expect("twirp response isn't valid JSON")
}

pub async fn read_err_body(body: Body) -> TwirpErrorResponse {
    read_json_body(body).await
}

// Hand written sample test server and client

pub struct TestAPIServer;

#[async_trait]
impl TestAPI for TestAPIServer {
    async fn ping(&self, req: PingRequest) -> Result<PingResponse, TwirpErrorResponse> {
        Ok(PingResponse { name: req.name })
    }

    async fn boom(&self, _: PingRequest) -> Result<PingResponse, TwirpErrorResponse> {
        Err(error::internal("boom!"))
    }
}

// Small test twirp services (this would usually be generated with twirp-build)
#[async_trait]
pub trait TestAPIClient {
    async fn ping(&self, req: PingRequest) -> Result<PingResponse>;
    async fn boom(&self, req: PingRequest) -> Result<PingResponse>;
}

#[async_trait]
impl TestAPIClient for Client {
    async fn ping(&self, req: PingRequest) -> Result<PingResponse> {
        let url = self.base_url.join("test.TestAPI/Ping")?;
        self.request(url, req).await
    }

    async fn boom(&self, _req: PingRequest) -> Result<PingResponse> {
        todo!()
    }
}

#[async_trait]
pub trait TestAPI {
    async fn ping(&self, req: PingRequest) -> Result<PingResponse, TwirpErrorResponse>;
    async fn boom(&self, req: PingRequest) -> Result<PingResponse, TwirpErrorResponse>;
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
