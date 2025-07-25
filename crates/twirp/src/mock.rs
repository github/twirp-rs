use http_body_util::BodyExt;

use crate::{malformed, serialize_proto_message, Request, Response, Result};

// TODO: Figure out `test-support` feature.
// TODO: Mock out other headers and extensions in the request and response?

// NOTE: For testing and mocking only.
pub async fn decode_request<I>(mut req: reqwest::Request) -> Result<Request<I>>
where
    I: prost::Message + Default,
{
    let url = req.url().clone();
    let headers = req.headers().clone();
    let body = std::mem::take(req.body_mut())
        .ok_or_else(|| malformed("failed to read the request body"))?
        .collect()
        .await?
        .to_bytes();
    let data = I::decode(body).map_err(|e| malformed(format!("failed to decode request: {e}")))?;
    let mut req = Request::builder().method("POST").uri(url.to_string());
    req.headers_mut()
        .expect("failed to get headers")
        .extend(headers);
    let req = req
        .body(data)
        .map_err(|e| malformed(format!("failed to build the request: {e}")))?;
    Ok(req)
}

// NOTE: For testing and mocking only.
pub fn encode_response<O>(resp: Response<O>) -> Result<reqwest::Response>
where
    O: prost::Message + Default,
{
    let mut resp = resp.map(serialize_proto_message);
    resp.headers_mut()
        .insert("Content-Type", "application/protobuf".try_into()?);
    Ok(reqwest::Response::from(resp))
}
