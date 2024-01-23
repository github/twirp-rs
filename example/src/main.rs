use std::net::SocketAddr;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use axum::routing::get;
use twirp::{invalid_argument, Router, TwirpErrorResponse};

pub mod service {
    pub mod haberdash {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
        }
    }
}
use service::haberdash::v1::{self as haberdash, MakeHatRequest, MakeHatResponse};

async fn ping() -> &'static str {
    "Pong\n"
}

#[tokio::main]
pub async fn main() {
    let api_impl = Arc::new(HaberdasherAPIServer {});
    let twirp_routes = Router::new().nest(haberdash::SERVICE_FQN, haberdash::router(api_impl));
    let app = Router::new()
        .nest("/twirp", twirp_routes)
        .route("/_ping", get(ping))
        .fallback(twirp::server::not_found_handler);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let tcp_listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind");
    println!("Listening on {addr}");
    if let Err(e) = axum::serve(tcp_listener, app).await {
        eprintln!("server error: {}", e);
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
