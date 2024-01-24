use std::sync::Arc;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::{service_fn, Service};
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use twirp::{invalid_argument, Body, GenericError, Router, TwirpErrorResponse};

pub mod service {
    pub mod haberdash {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
        }
    }
}
use service::haberdash::v1::{self as haberdash, MakeHatRequest, MakeHatResponse};

#[tokio::main]
pub async fn main() {
    let mut router = Router::default();
    let example = Arc::new(HaberdasherAPIServer {});
    haberdash::add_service(&mut router, example.clone());
    router.add_sync_handler(Method::GET, "/_ping", |_req| {
        Ok(Response::new(Body::from("Pong\n")))
    });
    println!("{router:?}");
    let router = Arc::new(router);
    let service = service_fn(move |req: Request<Incoming>| {
        let router = Arc::clone(&router);
        async move {
            let req = req.map(Body::new);
            twirp::serve(router, req).await
        }
    });

    let tcp_listener = TcpListener::bind("localhost:3000").await.unwrap();
    println!("Listening on localhost:3000");
    if let Err(e) = serve_forever(tcp_listener, service).await {
        eprintln!("server error: {}", e);
    }
}

async fn serve_forever<S>(tcp_listener: TcpListener, service: S) -> Result<(), std::io::Error>
where
    S: Clone + Service<Request<Incoming>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<GenericError>,
{
    loop {
        let (stream, _) = tcp_listener.accept().await?;
        let io = TokioIo::new(stream);
        let service = service.clone();
        let task = async move {
            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("test server: error serving connection: {err:#}");
            }
        };
        tokio::spawn(task);
    }
}

struct HaberdasherAPIServer;

#[async_trait]
impl haberdash::HaberdasherAPI for HaberdasherAPIServer {
    async fn make_hat(&self, req: MakeHatRequest) -> Result<MakeHatResponse, TwirpErrorResponse> {
        if req.inches == 0 {
            return Err(invalid_argument("inches"));
        }

        println!("got {:?}", req);
        let ts = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Ok(MakeHatResponse {
            color: "black".to_string(),
            name: "top hat".to_string(),
            size: req.inches,
            timestamp: Some(prost_wkt_types::Timestamp {
                seconds: ts.as_secs() as i64,
                nanos: 0,
            }),
        })
    }
}

#[cfg(test)]
mod test {
    use twirp::TwirpErrorCode;

    use crate::service::haberdash::v1::HaberdasherAPI;

    use super::*;

    #[tokio::test]
    async fn success() {
        let api = HaberdasherAPIServer {};
        let res = api.make_hat(MakeHatRequest { inches: 1 }).await;
        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.size, 1);
    }

    #[tokio::test]
    async fn invalid_request() {
        let api = HaberdasherAPIServer {};
        let res = api.make_hat(MakeHatRequest { inches: 0 }).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.code, TwirpErrorCode::InvalidArgument);
    }
}
