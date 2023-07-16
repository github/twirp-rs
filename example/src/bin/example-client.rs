use async_trait::async_trait;
use twirp::client::{request, HttpTwirpClient, TwirpClient, TwirpClientError};
use twirp::url::Url;
use twirp::GenericError;

pub mod service {
    pub mod haberdash {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
        }
    }
}

use service::haberdash::v1::{HaberdasherAPIClient, MakeHatRequest, MakeHatResponse};

#[tokio::main]
pub async fn main() -> Result<(), GenericError> {
    // basic client
    use service::haberdash::v1::HaberdasherAPIClient;
    let client = HttpTwirpClient::default(Url::parse("http://localhost:3000/twirp/")?)?;
    let resp = client.make_hat(MakeHatRequest { inches: 1 }).await;
    eprintln!("{:?}", resp);

    // custom client
    let client = CustomTwirpClient::new(Url::parse("http://xyz:3000/twirp/")?)?;
    let resp = client
        .make_hat("localhost", MakeHatRequest { inches: 1 })
        .await;
    eprintln!("{:?}", resp);
    Ok(())
}

/// `CustomTwirpClient` includes some extra state (hmac keys for auth) and wraps
/// each request in order to add some HTTP headers.
pub struct CustomTwirpClient {
    hmac_key: Option<String>,
    inner: HttpTwirpClient,
}

impl CustomTwirpClient {
    fn new(base_url: Url) -> Result<Self, TwirpClientError> {
        let client = HttpTwirpClient::default(base_url)?;
        Ok(CustomTwirpClient {
            hmac_key: None,
            inner: client,
        })
    }

    /// This version of `make_hat` allows dynamically setting the hostname per
    /// request. It demonstrates that it's possible to override another of the
    /// request cycle, if desired.
    async fn make_hat(
        &self,
        hostname: &str,
        req: MakeHatRequest,
    ) -> Result<MakeHatResponse, TwirpClientError> {
        let mut url = self.inner.make_hat_url(&self.inner.base_url)?;
        url.set_host(Some(hostname))?;
        self.request(url, req).await
    }
}

#[async_trait]
impl TwirpClient for CustomTwirpClient {
    async fn request<I, O>(&self, url: Url, body: I) -> twirp::client::Result<O>
    where
        I: prost::Message,
        O: prost::Message + Default,
    {
        let mut r = self.inner.client.post(url).header("Request-Id", "XYZ");
        if let Some(_hmac_key) = &self.hmac_key {
            r = r.header("Request-HMAC", "example:todo");
        }
        request(r, body).await
    }
}
