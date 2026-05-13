use std::env;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub api_token: String,
    pub bind_addr: String,
    pub output_dir: PathBuf,
    pub max_download_bytes: usize,
    pub download_timeout_secs: u64,
    pub max_body_bytes: usize,
    pub resize_workers: usize,
    pub jpeg_quality: u8,
    pub max_scale: f32,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let api_token = env::var("API_TOKEN")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("API_TOKEN env var is required and must be non-empty"))?;

        let bind_addr = env_string("BIND_ADDR", "0.0.0.0:8080");
        let output_dir = PathBuf::from(env_string("OUTPUT_DIR", "/data/images"));

        let max_download_bytes = env_parse::<usize>("MAX_DOWNLOAD_BYTES", 25 * 1024 * 1024)?;
        let download_timeout_secs = env_parse::<u64>("DOWNLOAD_TIMEOUT_SECS", 15)?;
        let max_body_bytes = env_parse::<usize>("MAX_BODY_BYTES", 1024 * 1024)?;

        let resize_workers = env_parse::<usize>("RESIZE_WORKERS", num_cpus::get().max(1))?;
        let resize_workers = resize_workers.max(1);

        let jpeg_quality = env_parse::<u8>("JPEG_QUALITY", 85)?;
        let jpeg_quality = jpeg_quality.clamp(1, 100);

        let max_scale = env_parse::<f32>("MAX_SCALE", 10.0)?;

        Ok(Self {
            api_token,
            bind_addr,
            output_dir,
            max_download_bytes,
            download_timeout_secs,
            max_body_bytes,
            resize_workers,
            jpeg_quality,
            max_scale,
        })
    }
}

fn env_string(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_parse<T>(key: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => v
            .trim()
            .parse::<T>()
            .map_err(|e| anyhow!("invalid value for {key}: {e}"))
            .with_context(|| format!("parsing env var {key}")),
        _ => Ok(default),
    }
}
