use std::time::UNIX_EPOCH;

use thiserror::Error;
use twirp::async_trait::async_trait;
use twirp::{
    internal, invalid_argument, GenericError, IntoTwirpResponse, Request, TwirpErrorResponse,
};

pub mod service {
    pub mod haberdash {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
        }
    }
}

use crate::service::haberdash::v1::{
    GetStatusRequest, GetStatusResponse, HaberdasherApi, MakeHatRequest, MakeHatResponse,
};

/// Demonstrates a client that uses a server implementation directly.
#[tokio::main]
pub async fn main() -> Result<(), GenericError> {
    let api_impl = HaberdasherApiServer {};
    let client = HaberdasherApiDirectClient(api_impl);

    let resp = client
        .make_hat(Request::new(MakeHatRequest { inches: 1 }))
        .await;
    eprintln!("{:?}", resp);

    Ok(())
}

#[derive(Clone)]
pub struct HaberdasherApiDirectClient<T>(pub T)
where
    T: HaberdasherApi;
#[twirp::async_trait::async_trait]
impl<T> HaberdasherApi for HaberdasherApiDirectClient<T>
where
    T: HaberdasherApi,
{
    async fn make_hat(
        &self,
        req: twirp::Request<MakeHatRequest>,
    ) -> Result<twirp::Response<MakeHatResponse>, twirp::TwirpErrorResponse> {
        let res = self
            .0
            .make_hat(req)
            .await
            .map_err(|err| err.into_twirp_response().into_body())?;
        Ok(res)
    }
    async fn get_status(
        &self,
        req: twirp::Request<GetStatusRequest>,
    ) -> Result<twirp::Response<GetStatusResponse>, twirp::TwirpErrorResponse> {
        let res = self
            .0
            .get_status(req)
            .await
            .map_err(|err| err.into_twirp_response().into_body())?;
        Ok(res)
        // Ok(self.0.get_status(req).await?)
    }
}

#[derive(Debug, Error)]
pub enum CustomError {
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Internal server error")]
    InternalServerError,
}

impl IntoTwirpResponse for CustomError {
    fn into_twirp_response(self) -> twirp::Response<TwirpErrorResponse> {
        match self {
            CustomError::InvalidArgument(msg) => invalid_argument(msg),
            CustomError::InternalServerError => internal("internal server error"),
        }
        .into_twirp_response()
    }
}

#[derive(Clone)]
struct HaberdasherApiServer;

#[async_trait]
impl HaberdasherApi for HaberdasherApiServer {
    async fn make_hat(
        &self,
        req: twirp::Request<MakeHatRequest>,
    ) -> Result<twirp::Response<MakeHatResponse>, twirp::TwirpErrorResponse> {
        let data = req.into_body();
        if data.inches == 0 {
            return Err(invalid_argument("inches must be greater than 0"));
        }

        let ts = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let resp = twirp::Response::new(MakeHatResponse {
            color: "black".to_string(),
            name: "top hat".to_string(),
            size: data.inches,
            timestamp: Some(prost_wkt_types::Timestamp {
                seconds: ts.as_secs() as i64,
                nanos: 0,
            }),
        });
        Ok(resp)
    }

    async fn get_status(
        &self,
        _req: twirp::Request<GetStatusRequest>,
    ) -> Result<twirp::Response<GetStatusResponse>, twirp::TwirpErrorResponse> {
        Ok(twirp::Response::new(GetStatusResponse {
            status: "making hats".to_string(),
        }))
    }
}
