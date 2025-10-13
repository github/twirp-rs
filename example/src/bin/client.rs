use twirp::async_trait::async_trait;
use twirp::client::{Client, ClientBuilder, Middleware, Next};
use twirp::url::Url;
use twirp::{GenericError, Request};

pub mod service {
    pub mod haberdash {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
        }
    }
    pub mod status {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/service.status.v1.rs"));
        }
    }
}

use service::haberdash::v1::{HaberdasherApi, MakeHatRequest};

/// You can run this end-to-end example by running both a server and a client and observing the requests/responses.
///
/// 1. Run the server:
/// ```sh
/// cargo run --bin advanced-server # OR cargo run --bin simple-server
/// ```
///
/// 2. In another shell, run the client:
/// ```sh
/// cargo run --bin client
/// ```
#[tokio::main]
pub async fn main() -> Result<(), GenericError> {
    // basic client
    let client = Client::from_base_url(Url::parse("http://localhost:3000/twirp/")?);
    let resp = client
        .make_hat(Request::new(MakeHatRequest { inches: 1 }))
        .await;
    eprintln!("{:?}", resp);

    // customize the client with middleware
    let client = ClientBuilder::new(Url::parse("http://xyz:3000/twirp/")?)
        .with_middleware(RequestHeaders { hmac_key: None })
        .with_middleware(PrintResponseHeaders {})
        .build();
    let resp = client
        .with_host("localhost")
        .make_hat(Request::new(MakeHatRequest { inches: 1 }))
        .await;
    eprintln!("{:?}", resp);

    Ok(())
}

struct RequestHeaders {
    hmac_key: Option<String>,
}

#[async_trait]
impl Middleware for RequestHeaders {
    async fn handle(
        &self,
        mut req: twirp::reqwest::Request,
        next: Next<'_>,
    ) -> twirp::Result<twirp::reqwest::Response> {
        req.headers_mut().append("x-request-id", "XYZ".try_into()?);
        if let Some(_hmac_key) = &self.hmac_key {
            req.headers_mut()
                .append("Request-HMAC", "example:todo".try_into()?);
        }
        eprintln!("Set headers: {req:?}");
        next.run(req).await
    }
}

struct PrintResponseHeaders;

#[async_trait]
impl Middleware for PrintResponseHeaders {
    async fn handle(
        &self,
        req: twirp::reqwest::Request,
        next: Next<'_>,
    ) -> twirp::Result<twirp::reqwest::Response> {
        let res = next.run(req).await?;
        eprintln!("Response headers: {res:?}");
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use crate::service::haberdash::v1::handler::HaberdasherApiHandler;
    use crate::service::haberdash::v1::{GetStatusRequest, GetStatusResponse, MakeHatResponse};
    use crate::service::status::v1::handler::StatusApiHandler;
    use crate::service::status::v1::{GetSystemStatusRequest, GetSystemStatusResponse, StatusApi};

    use super::*;

    #[tokio::test]
    async fn test_client_with_mocks() {
        let client = ClientBuilder::direct()
            .with_handler(HaberdasherApiHandler::new(Mock))
            .with_handler(StatusApiHandler::new(Mock))
            .build();
        let resp = client
            .make_hat(Request::new(MakeHatRequest { inches: 1 }))
            .await;
        eprintln!("{:?}", resp);
        assert!(resp.is_ok());
        assert_eq!(42, resp.unwrap().into_body().size);

        let resp = client
            .get_system_status(Request::new(GetSystemStatusRequest {}))
            .await;
        eprintln!("{:?}", resp);
        assert!(resp.is_ok());
        assert_eq!("ok", resp.unwrap().into_body().status);
    }

    struct Mock;

    #[async_trait]
    impl HaberdasherApi for Mock {
        async fn make_hat(
            &self,
            req: Request<MakeHatRequest>,
        ) -> twirp::Result<twirp::Response<MakeHatResponse>> {
            eprintln!("Mock make_hat called with: {:?}", req);
            Ok(twirp::Response::new(MakeHatResponse {
                size: 42,
                ..Default::default()
            }))
        }

        async fn get_status(
            &self,
            _req: Request<GetStatusRequest>,
        ) -> twirp::Result<twirp::Response<GetStatusResponse>> {
            todo!()
        }
    }

    #[async_trait]
    impl StatusApi for Mock {
        async fn get_system_status(
            &self,
            req: Request<GetSystemStatusRequest>,
        ) -> twirp::Result<twirp::Response<GetSystemStatusResponse>> {
            eprintln!("Mock get_system_status called with: {:?}", req);
            Ok(twirp::Response::new(GetSystemStatusResponse {
                status: "ok".into(),
            }))
        }
    }
}
