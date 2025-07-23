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
    let client = ClientBuilder::new(
        Url::parse("http://xyz:3000/twirp/")?,
        twirp::reqwest::Client::default(),
    )
    .with(RequestHeaders { hmac_key: None })
    .with(PrintResponseHeaders {})
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
    use std::sync::Arc;

    use crate::service::haberdash::v1::test::MockHaberdasherApiClient;
    use crate::service::haberdash::v1::{GetStatusRequest, GetStatusResponse, MakeHatResponse};

    use super::*;

    #[tokio::test]
    async fn test_client_with_mock() {
        let mock = Mock;
        let client = Client::for_test(MockHaberdasherApiClient::new(Arc::new(mock)));
        let resp = client
            .make_hat(Request::new(MakeHatRequest { inches: 1 }))
            .await;
        eprintln!("{:?}", resp);
        assert!(resp.is_ok());
        assert_eq!(42, resp.unwrap().into_body().size);
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

    // use twirp::client::MockHandler;
    // use twirp::reqwest;
    // use twirp::test::{decode_request, encode_response};

    // struct MockHaberdasherApiClient {
    //     inner: Arc<dyn HaberdasherApi>,
    // }

    // impl MockHaberdasherApiClient {
    //     pub fn new(inner: Arc<dyn HaberdasherApi>) -> Arc<Self> {
    //         Arc::new(Self { inner })
    //     }
    // }

    // #[async_trait]
    // impl MockHandler for MockHaberdasherApiClient {
    //     async fn handle(&self, req: reqwest::Request) -> twirp::Result<reqwest::Response> {
    //         let Some(segments) = req.url().path_segments() else {
    //             return Err(twirp::bad_route(format!(
    //                 "invalid request to {}: no path segments",
    //                 req.url()
    //             )));
    //         };
    //         let Some(path) = segments.last() else {
    //             return Err(twirp::bad_route(format!(
    //                 "invalid request to {}: no path",
    //                 req.url()
    //             )));
    //         };

    //         match path {
    //             "MakeHat" => {
    //                 encode_response(self.inner.make_hat(decode_request(req).await?).await?)
    //             }
    //             "GetStatus" => {
    //                 encode_response(self.inner.get_status(decode_request(req).await?).await?)
    //             }
    //             _ => Err(twirp::bad_route(format!("path '{path:?}' not found"))),
    //         }
    //     }
    // }
}
