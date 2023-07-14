use async_trait::async_trait;
use twirp::client::{request, TwirpClient, TwirpClientError};
use twirp::url::Url;
use twirp::GenericError;

pub mod service {
    pub mod haberdash {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
        }
    }
}

use service::haberdash::v1::{HaberdasherAPIClientExt, MakeHatRequest, MakeHatResponse};

#[tokio::main]
pub async fn main() -> Result<(), GenericError> {
    // basic client
    use service::haberdash::v1::HaberdasherAPIClient;
    let client = TwirpClient::default(Url::parse("http://localhost:3000")?)?;
    let resp = client.make_hat(MakeHatRequest { inches: 1 }).await;
    eprintln!("{:?}", resp);

    // custom client
    let client = CustomTwirpClient::new(Url::parse("http://xyz:3000")?)?;
    let resp = client
        .make_hat("localhost", MakeHatRequest { inches: 1 })
        .await;
    eprintln!("{:?}", resp);
    Ok(())
}

pub struct CustomTwirpClient {
    hmac_key: Option<String>,
    client: TwirpClient,
}

impl CustomTwirpClient {
    fn new(base_url: Url) -> Result<Self, TwirpClientError> {
        let client = TwirpClient::default(base_url)?;
        Ok(CustomTwirpClient {
            hmac_key: None,
            client,
        })
    }

    async fn make_hat(
        &self,
        hostname: &str,
        req: MakeHatRequest,
    ) -> Result<MakeHatResponse, TwirpClientError> {
        let mut url = self.make_hat_url(&self.client.base_url)?;
        url.set_host(Some(hostname))?;
        self.make_hat_with_url(url, req).await
    }
}

#[async_trait]
impl HaberdasherAPIClientExt for CustomTwirpClient {
    async fn make_hat_with_url(
        &self,
        url: Url,
        req: MakeHatRequest,
    ) -> Result<MakeHatResponse, TwirpClientError> {
        let mut r = self.client.client.post(url).header("Request-Id", "XYZ");
        if let Some(_hmac_key) = &self.hmac_key {
            r = r.header("Request-HMAC", "example:todo");
        }
        request(r, req).await
    }
}
