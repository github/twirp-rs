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

use service::haberdash::v1::{
    GetStatusRequest, GetStatusResponse, HaberdasherApi, MakeHatRequest, MakeHatResponse,
};

#[tokio::main]
pub async fn main() -> Result<(), GenericError> {
    // basic client
    let client = Client::from_base_url(Url::parse("http://localhost:3000/twirp/")?)?;
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
    .build()?;
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
    ) -> twirp::client::Result<twirp::reqwest::Response> {
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
    ) -> twirp::client::Result<twirp::reqwest::Response> {
        let res = next.run(req).await?;
        eprintln!("Response headers: {res:?}");
        Ok(res)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct MockHaberdasherApiClient;

#[async_trait]
impl HaberdasherApi for MockHaberdasherApiClient {
    type Error = twirp::client::ClientError;

    async fn make_hat(
        &self,
        _req: Request<MakeHatRequest>,
    ) -> Result<twirp::Response<MakeHatResponse>, Self::Error> {
        todo!()
    }

    async fn get_status(
        &self,
        _req: Request<GetStatusRequest>,
    ) -> Result<twirp::Response<GetStatusResponse>, Self::Error> {
        todo!()
    }
}
