use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::error::AppError;
use crate::services::resizer::{OutputFormat, ResizeRequest};
use crate::services::{downloader, resizer};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ResizePayload {
    pub url: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub scale: Option<f32>,
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MemoryReport {
    pub source_compressed_kb: f64,
    pub source_decoded_kb: f64,
    pub output_decoded_kb: f64,
    pub output_encoded_kb: f64,
    pub peak_estimate_kb: f64,
}

fn bytes_to_kb(bytes: u64) -> f64 {
    ((bytes as f64) / 1024.0 * 100.0).round() / 100.0
}

#[derive(Debug, Serialize)]
pub struct TimingReport {
    pub download_ms: u64,
    pub resize_ms: u64,
    pub save_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct ResizeResponse {
    pub path: String,
    pub filename: String,
    pub width: u32,
    pub height: u32,
    pub size_kb: f64,
    pub format: &'static str,
    pub content_type: &'static str,
    pub processing_time_ms: u64,
    pub timing: TimingReport,
    pub memory_kb: MemoryReport,
}

pub async fn resize(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ResizePayload>,
) -> Result<Json<ResizeResponse>, AppError> {
    if payload.url.trim().is_empty() {
        return Err(AppError::BadRequest("url is required".into()));
    }

    if payload.width.is_none() && payload.height.is_none() && payload.scale.is_none() {
        return Err(AppError::BadRequest(
            "at least one of width, height, or scale is required".into(),
        ));
    }

    let output_format = match payload.format.as_deref() {
        Some(s) if !s.trim().is_empty() => Some(OutputFormat::parse(s.trim())?),
        _ => None,
    };

    let total_start = Instant::now();

    let download_start = Instant::now();
    let (bytes, _content_type) = downloader::fetch(
        &state.http_client,
        &payload.url,
        state.config.max_download_bytes,
    )
    .await?;
    let download_ms = download_start.elapsed().as_millis() as u64;

    let source_compressed_bytes = bytes.len() as u64;

    let request = ResizeRequest {
        bytes,
        width: payload.width,
        height: payload.height,
        scale: payload.scale,
        output_format,
        jpeg_quality: state.config.jpeg_quality,
        max_scale: state.config.max_scale,
    };

    let resize_start = Instant::now();
    let result = resizer::resize_image(state.resize_semaphore.clone(), request).await?;
    let resize_ms = resize_start.elapsed().as_millis() as u64;

    let filename = format!("{}.{}", Uuid::new_v4(), result.format.extension());
    let full_path = state.config.output_dir.join(&filename);

    let save_start = Instant::now();
    tokio::fs::create_dir_all(&state.config.output_dir)
        .await
        .map_err(|e| AppError::Internal(format!("could not ensure output dir: {e}")))?;

    let mut file = tokio::fs::File::create(&full_path)
        .await
        .map_err(|e| AppError::Internal(format!("could not create output file: {e}")))?;
    file.write_all(&result.bytes)
        .await
        .map_err(|e| AppError::Internal(format!("could not write output file: {e}")))?;
    file.flush()
        .await
        .map_err(|e| AppError::Internal(format!("could not flush output file: {e}")))?;
    let save_ms = save_start.elapsed().as_millis() as u64;

    let size_bytes = result.bytes.len() as u64;
    let total_ms = total_start.elapsed().as_millis() as u64;

    let source_decoded_bytes = result.source_decoded_bytes();
    let output_decoded_bytes = result.output_decoded_bytes();
    let output_encoded_bytes = size_bytes;
    let peak_estimate_bytes = source_compressed_bytes
        .saturating_add(source_decoded_bytes)
        .saturating_add(output_decoded_bytes)
        .saturating_add(output_encoded_bytes);

    tracing::info!(
        path = %full_path.display(),
        width = result.width,
        height = result.height,
        size_kb = bytes_to_kb(size_bytes),
        format = result.format.extension(),
        total_ms,
        download_ms,
        resize_ms,
        save_ms,
        peak_memory_kb = bytes_to_kb(peak_estimate_bytes),
        "image resized"
    );

    Ok(Json(ResizeResponse {
        path: full_path.to_string_lossy().into_owned(),
        filename,
        width: result.width,
        height: result.height,
        size_kb: bytes_to_kb(size_bytes),
        format: result.format.extension(),
        content_type: result.format.content_type(),
        processing_time_ms: total_ms,
        timing: TimingReport {
            download_ms,
            resize_ms,
            save_ms,
            total_ms,
        },
        memory_kb: MemoryReport {
            source_compressed_kb: bytes_to_kb(source_compressed_bytes),
            source_decoded_kb: bytes_to_kb(source_decoded_bytes),
            output_decoded_kb: bytes_to_kb(output_decoded_bytes),
            output_encoded_kb: bytes_to_kb(output_encoded_bytes),
            peak_estimate_kb: bytes_to_kb(peak_estimate_bytes),
        },
    }))
}
