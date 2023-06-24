use std::sync::Arc;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Response, Server};
use twirp::{invalid_argument, GenericError, Router, TwirpErrorResponse};

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
    router.add_handler(Method::GET, "/_ping", |_req| {
        Ok(Response::new(Body::from("Pong\n")))
    });
    println!("{router:?}");
    let router = Arc::new(router);
    let service = make_service_fn(move |_| {
        let router = router.clone();
        async { Ok::<_, GenericError>(service_fn(move |req| twirp::serve(router.clone(), req))) }
    });

    let addr = ([127, 0, 0, 1], 3000).into();
    let server = Server::bind(&addr).serve(service);
    println!("Listening on {addr}");
    if let Err(e) = server.await {
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
