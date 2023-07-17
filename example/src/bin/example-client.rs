use async_trait::async_trait;
use twirp::client::{HttpTwirpClient, Middleware, Next, TwirpClient, TwirpClientBuilder};
use twirp::reqwest::{Request, Response};
use twirp::url::Url;
use twirp::GenericError;

pub mod service {
    pub mod haberdash {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
        }
    }
}

use service::haberdash::v1::MakeHatRequest;

#[tokio::main]
pub async fn main() -> Result<(), GenericError> {
    // basic client
    use service::haberdash::v1::HaberdasherAPIClient;
    let client = HttpTwirpClient::default(Url::parse("http://localhost:3000/twirp/")?)?;
    let resp = client.make_hat(MakeHatRequest { inches: 1 }).await;
    eprintln!("{:?}", resp);

    // customize the client with middleware
    let client = TwirpClientBuilder::new(Url::parse("http://xyz:3000/twirp/")?)
        .with(RequestHeaders { hmac_key: None })
        .build()?;
    let resp = client
        .with(hostname("localhost"))
        .make_hat(MakeHatRequest { inches: 1 })
        .await;
    eprintln!("{:?}", resp);

    Ok(())
}

fn hostname(hostname: &str) -> DynamicHostname {
    DynamicHostname(hostname.to_string())
}
struct DynamicHostname(String);

#[async_trait]
impl Middleware for DynamicHostname {
    async fn handle(&self, mut req: Request, next: Next<'_>) -> twirp::client::Result<Response> {
        req.url_mut().set_host(Some(&self.0))?;
        eprintln!("Set hostname");
        next.run(req).await
    }
}

struct RequestHeaders {
    hmac_key: Option<String>,
}

#[async_trait]
impl Middleware for RequestHeaders {
    async fn handle(&self, mut req: Request, next: Next<'_>) -> twirp::client::Result<Response> {
        req.headers_mut().append("Request_id", "XYZ".try_into()?);
        if let Some(_hmac_key) = &self.hmac_key {
            req.headers_mut()
                .append("Request-HMAC", "example:todo".try_into()?);
        }
        eprintln!("Set headers: {req:?}");
        next.run(req).await
    }
}
