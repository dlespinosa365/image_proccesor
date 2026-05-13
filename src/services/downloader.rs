use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use reqwest::Client;
use url::Url;

use crate::error::AppError;

pub async fn fetch(
    client: &Client,
    url: &str,
    max_bytes: usize,
) -> Result<(Bytes, Option<String>), AppError> {
    let parsed = Url::parse(url).map_err(|e| AppError::BadRequest(format!("invalid url: {e}")))?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(AppError::BadRequest(format!(
                "unsupported url scheme: {other}"
            )));
        }
    }

    let response = client
        .get(parsed)
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("request failed: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::Upstream(format!(
            "upstream returned status {}",
            response.status()
        )));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(len) = response.content_length() {
        if len as usize > max_bytes {
            return Err(AppError::PayloadTooLarge(format!(
                "remote image is {len} bytes (max {max_bytes})"
            )));
        }
    }

    let mut buf = BytesMut::with_capacity(
        response
            .content_length()
            .map(|l| (l as usize).min(max_bytes))
            .unwrap_or(64 * 1024),
    );

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::Upstream(format!("stream error: {e}")))?;
        if buf.len() + chunk.len() > max_bytes {
            return Err(AppError::PayloadTooLarge(format!(
                "remote image exceeds max size of {max_bytes} bytes"
            )));
        }
        buf.extend_from_slice(&chunk);
    }

    Ok((buf.freeze(), content_type))
}
