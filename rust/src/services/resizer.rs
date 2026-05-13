use std::sync::Arc;

use bytes::Bytes;
use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, ImageEncoder, ImageFormat};
use tokio::sync::Semaphore;

use crate::error::AppError;

const MAX_OUTPUT_DIMENSION: u32 = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Jpeg,
    Png,
    Webp,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "jpg",
            OutputFormat::Png => "png",
            OutputFormat::Webp => "webp",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "image/jpeg",
            OutputFormat::Png => "image/png",
            OutputFormat::Webp => "image/webp",
        }
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value.to_ascii_lowercase().as_str() {
            "jpeg" | "jpg" => Ok(OutputFormat::Jpeg),
            "png" => Ok(OutputFormat::Png),
            "webp" => Ok(OutputFormat::Webp),
            other => Err(AppError::BadRequest(format!(
                "unsupported output format: {other}"
            ))),
        }
    }

    fn from_image_format(fmt: ImageFormat) -> Option<Self> {
        match fmt {
            ImageFormat::Jpeg => Some(OutputFormat::Jpeg),
            ImageFormat::Png => Some(OutputFormat::Png),
            ImageFormat::WebP => Some(OutputFormat::Webp),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ResizeRequest {
    pub bytes: Bytes,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub scale: Option<f32>,
    pub output_format: Option<OutputFormat>,
    pub jpeg_quality: u8,
    pub max_scale: f32,
}

#[derive(Debug)]
pub struct ResizeResult {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: OutputFormat,
    pub source_width: u32,
    pub source_height: u32,
}

impl ResizeResult {
    pub fn source_decoded_bytes(&self) -> u64 {
        (self.source_width as u64) * (self.source_height as u64) * 4
    }

    pub fn output_decoded_bytes(&self) -> u64 {
        (self.width as u64) * (self.height as u64) * 4
    }
}

pub async fn resize_image(
    semaphore: Arc<Semaphore>,
    request: ResizeRequest,
) -> Result<ResizeResult, AppError> {
    let permit = semaphore
        .acquire_owned()
        .await
        .map_err(|e| AppError::Internal(format!("semaphore closed: {e}")))?;

    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        resize_blocking(request)
    })
    .await
    .map_err(|e| AppError::Internal(format!("worker join error: {e}")))?;

    result
}

fn resize_blocking(req: ResizeRequest) -> Result<ResizeResult, AppError> {
    let detected_format = image::guess_format(&req.bytes).ok();

    let img = image::load_from_memory(&req.bytes).map_err(|e| {
        AppError::UnsupportedMediaType(format!("could not decode source image: {e}"))
    })?;

    let src_width = img.width();
    let src_height = img.height();
    if src_width == 0 || src_height == 0 {
        return Err(AppError::UnsupportedMediaType(
            "source image has zero dimension".into(),
        ));
    }

    let output_format = req.output_format.unwrap_or_else(|| {
        detected_format
            .and_then(OutputFormat::from_image_format)
            .unwrap_or(OutputFormat::Png)
    });

    let (target_w, target_h) = calculate_target(
        src_width,
        src_height,
        req.width,
        req.height,
        req.scale,
        req.max_scale,
    )?;

    let rgba = img.to_rgba8();
    let (src_w, src_h) = (rgba.width(), rgba.height());

    let src_fir = FirImage::from_vec_u8(src_w, src_h, rgba.into_raw(), PixelType::U8x4)
        .map_err(|e| AppError::Internal(format!("invalid source buffer: {e}")))?;

    let mut dst_fir = FirImage::new(target_w, target_h, PixelType::U8x4);

    let mut resizer = Resizer::new();
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));

    resizer
        .resize(&src_fir, &mut dst_fir, &options)
        .map_err(|e| AppError::Internal(format!("resize failed: {e}")))?;

    let dst_buffer = dst_fir.into_vec();

    let mut out: Vec<u8> = Vec::with_capacity(64 * 1024);

    match output_format {
        OutputFormat::Jpeg => {
            let rgba_image = image::RgbaImage::from_raw(target_w, target_h, dst_buffer)
                .ok_or_else(|| AppError::Internal("failed to build resized rgba image".into()))?;
            let rgb = image::DynamicImage::ImageRgba8(rgba_image).to_rgb8();
            JpegEncoder::new_with_quality(&mut out, req.jpeg_quality)
                .write_image(
                    rgb.as_raw(),
                    target_w,
                    target_h,
                    ExtendedColorType::Rgb8,
                )
                .map_err(|e| AppError::Internal(format!("jpeg encode failed: {e}")))?;
        }
        OutputFormat::Png => {
            PngEncoder::new(&mut out)
                .write_image(&dst_buffer, target_w, target_h, ExtendedColorType::Rgba8)
                .map_err(|e| AppError::Internal(format!("png encode failed: {e}")))?;
        }
        OutputFormat::Webp => {
            WebPEncoder::new_lossless(&mut out)
                .write_image(&dst_buffer, target_w, target_h, ExtendedColorType::Rgba8)
                .map_err(|e| AppError::Internal(format!("webp encode failed: {e}")))?;
        }
    }

    Ok(ResizeResult {
        bytes: out,
        width: target_w,
        height: target_h,
        format: output_format,
        source_width: src_width,
        source_height: src_height,
    })
}

fn calculate_target(
    src_w: u32,
    src_h: u32,
    width: Option<u32>,
    height: Option<u32>,
    scale: Option<f32>,
    max_scale: f32,
) -> Result<(u32, u32), AppError> {
    if let Some(s) = scale {
        if !s.is_finite() || s <= 0.0 {
            return Err(AppError::BadRequest(
                "scale must be a positive finite number".into(),
            ));
        }
        if s > max_scale {
            return Err(AppError::BadRequest(format!(
                "scale exceeds the maximum allowed value of {max_scale}"
            )));
        }
        let w = ((src_w as f32) * s).round().max(1.0) as u32;
        let h = ((src_h as f32) * s).round().max(1.0) as u32;
        return validate_dimensions(w, h);
    }

    match (width, height) {
        (Some(w), Some(h)) => {
            validate_dimensions(w, h)
        }
        (Some(w), None) => {
            if w == 0 {
                return Err(AppError::BadRequest("width must be greater than zero".into()));
            }
            let ratio = w as f64 / src_w as f64;
            let h = ((src_h as f64) * ratio).round().max(1.0) as u32;
            validate_dimensions(w, h)
        }
        (None, Some(h)) => {
            if h == 0 {
                return Err(AppError::BadRequest(
                    "height must be greater than zero".into(),
                ));
            }
            let ratio = h as f64 / src_h as f64;
            let w = ((src_w as f64) * ratio).round().max(1.0) as u32;
            validate_dimensions(w, h)
        }
        (None, None) => Err(AppError::BadRequest(
            "at least one of width, height, or scale is required".into(),
        )),
    }
}

fn validate_dimensions(w: u32, h: u32) -> Result<(u32, u32), AppError> {
    if w == 0 || h == 0 {
        return Err(AppError::BadRequest(
            "target width and height must be greater than zero".into(),
        ));
    }
    if w > MAX_OUTPUT_DIMENSION || h > MAX_OUTPUT_DIMENSION {
        return Err(AppError::BadRequest(format!(
            "target dimensions exceed the maximum of {MAX_OUTPUT_DIMENSION}px"
        )));
    }
    Ok((w, h))
}
