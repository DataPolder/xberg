use crate::ocr::error::OcrError;
use crate::types::ImagePreprocessingConfig;
use xberg_tesseract::Pix;

const ADAPTIVE_TILE_SIZE: i32 = 32;
const CONTRAST_GAMMA: f32 = 0.5;
const CONTRAST_INPUT_MIN: i32 = 40;
const CONTRAST_INPUT_MAX: i32 = 220;
const DARK_BACKGROUND_MEAN_THRESHOLD: f64 = 100.0;
const DENOISE_KERNEL_SIZE: i32 = 3;
const LIGHT_PIXEL_VALUE_THRESHOLD: u8 = 180;
const MIN_LIGHT_PIXEL_FRACTION_FOR_INVERT: f64 = 0.01;
const POLARITY_SAMPLE_STRIDE: i32 = 4;
const SAUVOLA_FACTOR: f32 = 0.35;
const SAUVOLA_MAX_TILE_DIMENSION: i32 = 512;
const SAUVOLA_WINDOW_HALF_SIZE: i32 = 15;

pub(crate) fn should_invert_for_polarity(mean_gray: f64, light_fraction: f64, force_invert: bool) -> bool {
    force_invert
        || (mean_gray < DARK_BACKGROUND_MEAN_THRESHOLD && light_fraction >= MIN_LIGHT_PIXEL_FRACTION_FOR_INVERT)
}

pub(crate) fn preprocess_pix(pix: Pix, config: &ImagePreprocessingConfig) -> Result<Pix, OcrError> {
    if !matches!(
        config.binarization_method.to_ascii_lowercase().as_str(),
        "otsu" | "adaptive" | "sauvola"
    ) {
        return Err(OcrError::InvalidConfiguration(format!(
            "Invalid binarization method '{}'. Must be one of: otsu, adaptive, sauvola",
            config.binarization_method
        )));
    }

    let gray = pix
        .to_grayscale()
        .map_err(preprocessing_error("convert to grayscale"))?;
    let polarity_stats = gray
        .grayscale_stats(LIGHT_PIXEL_VALUE_THRESHOLD, POLARITY_SAMPLE_STRIDE)
        .ok();
    let should_invert = match polarity_stats {
        Some((mean, light_fraction)) => should_invert_for_polarity(mean, light_fraction, config.invert_colors),
        None => config.invert_colors,
    };

    let mut processed = if should_invert {
        gray.invert().map_err(preprocessing_error("invert colors"))?
    } else {
        gray
    };

    if config.denoise {
        processed = apply_optional(processed, "denoise", |source| {
            source.median_filter(DENOISE_KERNEL_SIZE, DENOISE_KERNEL_SIZE)
        });
    }
    if config.contrast_enhance {
        processed = apply_optional(processed, "enhance contrast", enhance_contrast);
    }

    processed = apply_optional(processed, "binarize", |source| {
        binarize(source, &config.binarization_method)
    });
    if config.deskew {
        processed = apply_optional(processed, "deskew", Pix::deskew);
    }
    Ok(processed)
}

fn enhance_contrast(source: &Pix) -> xberg_tesseract::Result<Pix> {
    let normalized = source.background_normalize()?;
    normalized.contrast_stretch(CONTRAST_GAMMA, CONTRAST_INPUT_MIN, CONTRAST_INPUT_MAX)
}

fn binarize(pix: &Pix, method: &str) -> xberg_tesseract::Result<Pix> {
    match method.to_ascii_lowercase().as_str() {
        "otsu" => pix.otsu_threshold(),
        "adaptive" if pix.width().min(pix.height()) >= ADAPTIVE_TILE_SIZE => {
            pix.adaptive_threshold(ADAPTIVE_TILE_SIZE, ADAPTIVE_TILE_SIZE)
        }
        "adaptive" => pix.otsu_threshold(),
        "sauvola" if pix.width().min(pix.height()) > SAUVOLA_WINDOW_HALF_SIZE * 2 + 2 => {
            let tile_columns = tile_count(pix.width());
            let tile_rows = tile_count(pix.height());
            pix.sauvola_threshold(SAUVOLA_WINDOW_HALF_SIZE, SAUVOLA_FACTOR, tile_columns, tile_rows)
        }
        "sauvola" => pix.otsu_threshold(),
        _ => Err(xberg_tesseract::TesseractError::InvalidParameterError),
    }
}

fn tile_count(dimension: i32) -> i32 {
    dimension
        .saturating_add(SAUVOLA_MAX_TILE_DIMENSION - 1)
        .checked_div(SAUVOLA_MAX_TILE_DIMENSION)
        .unwrap_or(1)
        .max(1)
}

fn apply_optional(
    source: Pix,
    operation: &'static str,
    transform: impl FnOnce(&Pix) -> xberg_tesseract::Result<Pix>,
) -> Pix {
    match transform(&source) {
        Ok(transformed) => transformed,
        Err(error) => {
            tracing::warn!(operation, %error, "OCR image preprocessing step failed; retaining prior raster");
            source
        }
    }
}

fn preprocessing_error(operation: &'static str) -> impl FnOnce(xberg_tesseract::TesseractError) -> OcrError {
    move |error| OcrError::ProcessingFailed(format!("Failed to {operation}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_reject_unknown_binarization_method() {
        let config = ImagePreprocessingConfig {
            binarization_method: "unknown".to_string(),
            ..Default::default()
        };
        let pix = Pix::from_raw_rgb(&vec![200; 64 * 64 * 3], 64, 64).unwrap();

        let result = preprocess_pix(pix, &config);

        assert!(matches!(result, Err(OcrError::InvalidConfiguration(_))));
    }
}
