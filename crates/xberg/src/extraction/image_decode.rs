use std::io::Cursor;

use image::{ImageDecoder, ImageFormat, ImageReader};

use crate::error::{Result, XbergError};
use crate::extractors::security::SecurityLimits;

#[derive(Clone, Copy)]
pub(crate) struct ImageDecodeBudget {
    max_decoded_bytes: u64,
}

impl ImageDecodeBudget {
    pub(crate) fn from_security_limits(limits: &SecurityLimits) -> Self {
        Self {
            max_decoded_bytes: u64::try_from(limits.max_content_size).unwrap_or(u64::MAX),
        }
    }

    pub(crate) fn validate(self, width: u32, height: u32, decoded_bytes: u64) -> Result<()> {
        let pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or_else(|| image_dimension_error(width, height, decoded_bytes, self.max_decoded_bytes))?;
        if width == 0 || height == 0 || pixels > self.max_decoded_bytes || decoded_bytes > self.max_decoded_bytes {
            return Err(image_dimension_error(
                width,
                height,
                decoded_bytes,
                self.max_decoded_bytes,
            ));
        }
        Ok(())
    }
}

pub(crate) fn image_dimension_error(width: u32, height: u32, decoded_bytes: u64, max_decoded_bytes: u64) -> XbergError {
    XbergError::Validation {
        message: format!(
            "Image dimensions {width}x{height} require {decoded_bytes} decoded bytes, exceeding or invalid under \
             security_limits.max_content_size ({max_decoded_bytes} bytes)"
        ),
        source: None,
    }
}

#[cfg(any(feature = "ocr", feature = "ocr-wasm", feature = "ocr-pipeline", feature = "heic"))]
pub(crate) fn decoded_byte_count(width: u32, height: u32, bytes_per_pixel: u64) -> Result<u64> {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .ok_or_else(|| image_dimension_error(width, height, u64::MAX, u64::MAX))
}

#[cfg(feature = "heic")]
pub(crate) fn copy_decoded_rows(
    data: &[u8],
    stride: usize,
    width: u32,
    height: u32,
    bytes_per_pixel: u64,
) -> Result<Vec<u8>> {
    let row_bytes = usize::try_from(decoded_byte_count(width, 1, bytes_per_pixel)?)
        .map_err(|error| XbergError::parsing(format!("Decoded image row size is not addressable: {error}")))?;
    let buffer_bytes = usize::try_from(decoded_byte_count(width, height, bytes_per_pixel)?)
        .map_err(|error| XbergError::parsing(format!("Decoded image buffer size is not addressable: {error}")))?;
    let row_count = usize::try_from(height)
        .map_err(|error| XbergError::parsing(format!("Decoded image height is not addressable: {error}")))?;
    let mut packed = Vec::new();
    packed
        .try_reserve_exact(buffer_bytes)
        .map_err(|error| XbergError::parsing(format!("Failed to reserve decoded image buffer: {error}")))?;
    for row in 0..row_count {
        let start = row
            .checked_mul(stride)
            .ok_or_else(|| XbergError::parsing("Decoded image row offset overflowed".to_string()))?;
        let end = start
            .checked_add(row_bytes)
            .ok_or_else(|| XbergError::parsing("Decoded image row end overflowed".to_string()))?;
        let row = data.get(start..end).ok_or_else(|| {
            XbergError::parsing("Decoded image plane is shorter than declared dimensions".to_string())
        })?;
        packed.extend_from_slice(row);
    }
    Ok(packed)
}

fn image_decode_limits(budget: ImageDecodeBudget) -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(budget.max_decoded_bytes);
    limits
}

fn map_image_decode_error(error: image::ImageError) -> XbergError {
    if matches!(error, image::ImageError::Limits(_)) {
        XbergError::Validation {
            message: format!("Image exceeds security_limits.max_content_size while decoding: {error}"),
            source: Some(Box::new(error)),
        }
    } else {
        XbergError::parsing(format!("Failed to decode image: {error}"))
    }
}

fn probe_standard_image(
    bytes: &[u8],
    budget: ImageDecodeBudget,
    format: Option<ImageFormat>,
) -> Result<(u32, u32, ImageFormat)> {
    let mut reader = match format {
        Some(format) => ImageReader::with_format(Cursor::new(bytes), format),
        None => ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| XbergError::parsing(format!("Failed to read image format: {error}")))?,
    };
    let format = reader
        .format()
        .ok_or_else(|| XbergError::parsing("Could not determine image format".to_string()))?;
    reader.limits(image_decode_limits(budget));
    let decoder = reader.into_decoder().map_err(map_image_decode_error)?;
    let (width, height) = decoder.dimensions();
    budget.validate(width, height, decoder.total_bytes())?;
    Ok((width, height, format))
}

#[cfg(any(feature = "ocr", feature = "ocr-wasm", feature = "ocr-pipeline"))]
pub(crate) fn probe_standard_image_with_security_limits(
    bytes: &[u8],
    limits: &SecurityLimits,
) -> Result<(u32, u32, image::ImageFormat)> {
    probe_standard_image(bytes, ImageDecodeBudget::from_security_limits(limits), None)
}

pub(crate) fn decode_standard_image_with_default_security_limits(bytes: &[u8]) -> Result<image::DynamicImage> {
    decode_standard_image_with_security_limits(bytes, &SecurityLimits::default())
}

#[cfg(feature = "image-encode")]
pub(crate) fn decode_standard_image_with_format_and_default_security_limits(
    bytes: &[u8],
    format: ImageFormat,
) -> Result<image::DynamicImage> {
    decode_standard_image(bytes, &SecurityLimits::default(), Some(format))
}

pub(crate) fn decode_standard_image_with_security_limits(
    bytes: &[u8],
    limits: &SecurityLimits,
) -> Result<image::DynamicImage> {
    decode_standard_image(bytes, limits, None)
}

fn decode_standard_image(
    bytes: &[u8],
    limits: &SecurityLimits,
    format: Option<ImageFormat>,
) -> Result<image::DynamicImage> {
    let budget = ImageDecodeBudget::from_security_limits(limits);
    let (_, _, format) = probe_standard_image(bytes, budget, format)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(image_decode_limits(budget));
    reader.decode().map_err(map_image_decode_error)
}

#[cfg(feature = "candle-glm-ocr")]
pub(crate) fn validate_standard_image_with_default_security_limits(bytes: &[u8]) -> Result<()> {
    validate_standard_image_with_security_limits(bytes, &SecurityLimits::default())
}

#[cfg(feature = "candle-glm-ocr")]
fn validate_standard_image_with_security_limits(bytes: &[u8], limits: &SecurityLimits) -> Result<()> {
    probe_standard_image(bytes, ImageDecodeBudget::from_security_limits(limits), None).map(|_| ())
}

#[cfg(test)]
pub(crate) fn bmp_with_declared_dimensions(width: u32, height: u32) -> Vec<u8> {
    use image::ImageEncoder;

    let mut bytes = Vec::new();
    image::codecs::bmp::BmpEncoder::new(&mut bytes)
        .write_image(&[255_u8, 255, 255], 1, 1, image::ExtendedColorType::Rgb8)
        .expect("encode the BMP control");
    bytes[18..22].copy_from_slice(&width.to_le_bytes());
    bytes[22..26].copy_from_slice(&height.to_le_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    fn rust_sources(root: &Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(root).expect("read source directory") {
            let path = entry.expect("read source entry").path();
            if path.is_dir() {
                rust_sources(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    #[test]
    fn direct_image_decode_api_calls_are_audited() {
        let expected = BTreeMap::from([
            ("candle_ocr/ocr_result.rs", 1),
            ("core/image_encode.rs", 1),
            ("extraction/heif.rs", 2),
            ("extraction/image_decode.rs", 3),
            ("extractors/image.rs", 1),
            ("extractors/pdf/layout_runner.rs", 1),
            ("extractors/pdf/mod.rs", 2),
            ("extractors/pdf/ocr.rs", 7),
            ("llm/vlm_ocr.rs", 1),
            ("ocr/tesseract_wasm_backend.rs", 2),
            ("paddle_ocr/backend.rs", 1),
            ("pdf/native/images.rs", 2),
            ("pdf/render.rs", 3),
        ]);
        let patterns = [
            ["image::load_", "from_memory"].concat(),
            ["Image", "Reader::"].concat(),
            ["Image::from_", "bytes("].concat(),
            [".decode(&", "handle"].concat(),
        ];
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_sources(&source_root, &mut files);
        let mut actual = BTreeMap::new();
        for path in files {
            let source = std::fs::read_to_string(&path).expect("read Rust source");
            let count = source
                .lines()
                .filter(|line| {
                    !line.trim_start().starts_with("//") && patterns.iter().any(|pattern| line.contains(pattern))
                })
                .count();
            if count > 0 {
                let relative = path.strip_prefix(&source_root).expect("source path under root");
                actual.insert(relative.to_string_lossy().replace('\\', "/"), count);
            }
        }
        let expected = expected
            .into_iter()
            .map(|(path, count)| (path.to_string(), count))
            .collect();
        assert_eq!(
            actual, expected,
            "a direct image decode call was added or removed without audit"
        );
    }
}
