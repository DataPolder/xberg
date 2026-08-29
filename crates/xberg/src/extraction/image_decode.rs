use std::io::Cursor;

use image::{ColorType, ImageDecoder, ImageFormat, ImageReader};

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

#[derive(Clone, Copy)]
struct StandardImageProbe {
    width: u32,
    height: u32,
    format: ImageFormat,
    color_type: ColorType,
    decoded_bytes: u64,
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
) -> Result<StandardImageProbe> {
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
    let decoded_bytes = decoder.total_bytes();
    budget.validate(width, height, decoded_bytes)?;
    Ok(StandardImageProbe {
        width,
        height,
        format,
        color_type: decoder.color_type(),
        decoded_bytes,
    })
}

pub(crate) fn probe_standard_image_with_security_limits(
    bytes: &[u8],
    limits: &SecurityLimits,
) -> Result<(u32, u32, image::ImageFormat)> {
    let probe = probe_standard_image(bytes, ImageDecodeBudget::from_security_limits(limits), None)?;
    Ok((probe.width, probe.height, probe.format))
}

pub(crate) fn probe_standard_image_with_default_security_limits(
    bytes: &[u8],
) -> Result<(u32, u32, image::ImageFormat)> {
    probe_standard_image_with_security_limits(bytes, &SecurityLimits::default())
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

pub(crate) fn validate_dynamic_image_additional_live_bytes(
    image: &image::DynamicImage,
    limits: &SecurityLimits,
    additional_bytes_per_pixel: u64,
    fixed_additional_bytes: u64,
) -> Result<()> {
    let width = image.width();
    let height = image.height();
    let live_bytes =
        u64::try_from(image.as_bytes().len()).map_err(|_| image_dimension_error(width, height, u64::MAX, u64::MAX))?;
    let additional_bytes = decoded_byte_count(width, height, additional_bytes_per_pixel)?
        .checked_add(fixed_additional_bytes)
        .ok_or_else(|| image_dimension_error(width, height, u64::MAX, u64::MAX))?;
    let peak_bytes = live_bytes
        .checked_add(additional_bytes)
        .ok_or_else(|| image_dimension_error(width, height, u64::MAX, u64::MAX))?;
    ImageDecodeBudget::from_security_limits(limits).validate(width, height, peak_bytes)
}

fn decode_standard_image(
    bytes: &[u8],
    limits: &SecurityLimits,
    format: Option<ImageFormat>,
) -> Result<image::DynamicImage> {
    let budget = ImageDecodeBudget::from_security_limits(limits);
    let probe = probe_standard_image(bytes, budget, format)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), probe.format);
    reader.limits(image_decode_limits(budget));
    reader.decode().map_err(map_image_decode_error)
}

fn conversion_peak_bytes(probe: StandardImageProbe, target: ColorType, additional_live_bytes: u64) -> Result<u64> {
    let target_bytes = decoded_byte_count(probe.width, probe.height, u64::from(target.bytes_per_pixel()))?;
    let conversion_peak = if probe.color_type == target {
        target_bytes
    } else {
        probe
            .decoded_bytes
            .checked_add(target_bytes)
            .ok_or_else(|| image_dimension_error(probe.width, probe.height, u64::MAX, u64::MAX))?
    };
    let post_conversion_peak = target_bytes
        .checked_add(additional_live_bytes)
        .ok_or_else(|| image_dimension_error(probe.width, probe.height, u64::MAX, u64::MAX))?;
    Ok(conversion_peak.max(post_conversion_peak))
}

fn decode_standard_rgb8(bytes: &[u8], limits: &SecurityLimits, additional_live_bytes: u64) -> Result<image::RgbImage> {
    let budget = ImageDecodeBudget::from_security_limits(limits);
    let probe = probe_standard_image(bytes, budget, None)?;
    let peak_bytes = conversion_peak_bytes(probe, ColorType::Rgb8, additional_live_bytes)?;
    budget.validate(probe.width, probe.height, peak_bytes)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), probe.format);
    reader.limits(image_decode_limits(budget));
    reader
        .decode()
        .map_err(map_image_decode_error)
        .map(image::DynamicImage::into_rgb8)
}

pub(crate) fn decode_standard_rgb8_with_security_limits(
    bytes: &[u8],
    limits: &SecurityLimits,
) -> Result<image::RgbImage> {
    decode_standard_rgb8(bytes, limits, 0)
}

pub(crate) fn decode_standard_rgb8_with_default_security_limits(bytes: &[u8]) -> Result<image::RgbImage> {
    decode_standard_rgb8_with_security_limits(bytes, &SecurityLimits::default())
}

pub(crate) fn decode_standard_rgb8_with_additional_live_bytes_and_default_security_limits(
    bytes: &[u8],
    additional_live_bytes: u64,
) -> Result<image::RgbImage> {
    decode_standard_rgb8(bytes, &SecurityLimits::default(), additional_live_bytes)
}

pub(crate) fn decode_standard_rgb8_with_same_sized_output_and_default_security_limits(
    bytes: &[u8],
) -> Result<image::RgbImage> {
    let limits = SecurityLimits::default();
    let budget = ImageDecodeBudget::from_security_limits(&limits);
    let probe = probe_standard_image(bytes, budget, None)?;
    let rgb_bytes = decoded_byte_count(probe.width, probe.height, u64::from(ColorType::Rgb8.bytes_per_pixel()))?;
    decode_standard_rgb8(bytes, &limits, rgb_bytes)
}

fn decode_standard_luma8_with_security_limits(bytes: &[u8], limits: &SecurityLimits) -> Result<image::GrayImage> {
    let budget = ImageDecodeBudget::from_security_limits(limits);
    let probe = probe_standard_image(bytes, budget, None)?;
    let peak_bytes = conversion_peak_bytes(probe, ColorType::L8, 0)?;
    budget.validate(probe.width, probe.height, peak_bytes)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), probe.format);
    reader.limits(image_decode_limits(budget));
    reader
        .decode()
        .map_err(map_image_decode_error)
        .map(image::DynamicImage::into_luma8)
}

pub(crate) fn decode_standard_luma8_with_default_security_limits(bytes: &[u8]) -> Result<image::GrayImage> {
    decode_standard_luma8_with_security_limits(bytes, &SecurityLimits::default())
}

pub(crate) fn standard_image_is_single_frame(bytes: &[u8], mime_type: &str) -> bool {
    let cursor = Cursor::new(bytes);
    match mime_type {
        "image/png" => image::codecs::png::PngDecoder::new(cursor)
            .and_then(|decoder| decoder.is_apng())
            .is_ok_and(|is_animated| !is_animated),
        "image/webp" => image::codecs::webp::WebPDecoder::new(cursor).is_ok_and(|decoder| !decoder.has_animation()),
        #[cfg(feature = "ocr")]
        "image/tiff" | "image/x-tiff" => {
            tiff::decoder::Decoder::new(cursor).is_ok_and(|decoder| !decoder.more_images())
        }
        _ => false,
    }
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

    use image::ImageEncoder;

    use super::*;

    fn grayscale_png(width: u32, height: u32) -> Vec<u8> {
        let pixels = vec![127_u8; usize::try_from(u64::from(width) * u64::from(height)).unwrap()];
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&pixels, width, height, image::ExtendedColorType::L8)
            .expect("encode grayscale PNG");
        bytes
    }

    fn decode_for_encode_under_test(
        bytes: &[u8],
        limits: &SecurityLimits,
        additional_bytes_per_pixel: u64,
    ) -> Result<image::DynamicImage> {
        let image = decode_standard_image_with_security_limits(bytes, limits)?;
        validate_dynamic_image_additional_live_bytes(&image, limits, additional_bytes_per_pixel, 0)?;
        Ok(image)
    }

    #[test]
    fn encode_decode_rejects_peak_when_output_buffer_exceeds_budget() {
        let bytes = grayscale_png(10, 10);
        let limits = SecurityLimits {
            max_content_size: 399,
            ..Default::default()
        };

        let error = decode_for_encode_under_test(&bytes, &limits, 3)
            .expect_err("100-byte luma plus 300-byte encoded-output budget must exceed 399 bytes");

        assert!(matches!(error, XbergError::Validation { .. }));
    }

    #[test]
    fn rgb_decode_rejects_peak_when_source_alone_fits() {
        let bytes = grayscale_png(10, 10);
        let limits = SecurityLimits {
            max_content_size: 399,
            ..Default::default()
        };

        let error = decode_standard_rgb8_with_security_limits(&bytes, &limits)
            .expect_err("100-byte luma plus 300-byte RGB conversion must exceed 399 bytes");

        assert!(matches!(error, XbergError::Validation { .. }));
        assert!(error.to_string().contains("400 decoded bytes"));
    }

    #[test]
    fn rgb_decode_accepts_peak_at_exact_budget() {
        let bytes = grayscale_png(10, 10);
        let limits = SecurityLimits {
            max_content_size: 400,
            ..Default::default()
        };

        let rgb = decode_standard_rgb8_with_security_limits(&bytes, &limits)
            .expect("the exact source-plus-RGB peak must fit");

        assert_eq!(rgb.dimensions(), (10, 10));
        assert_eq!(rgb.as_raw().len(), 300);
    }

    #[test]
    fn luma_decode_rejects_peak_when_rgba_source_alone_fits() {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&[0_u8; 400], 10, 10, image::ExtendedColorType::Rgba8)
            .expect("encode RGBA PNG");
        let limits = SecurityLimits {
            max_content_size: 499,
            ..Default::default()
        };

        let error = decode_standard_luma8_with_security_limits(&bytes, &limits)
            .expect_err("400-byte RGBA plus 100-byte luma conversion must exceed 499 bytes");

        assert!(matches!(error, XbergError::Validation { .. }));
        assert!(error.to_string().contains("500 decoded bytes"));
    }

    #[test]
    fn source_audit_detects_aliases_multiline_calls_and_decoder_families() {
        let source = r#"
            use image::load_from_memory as decode_pixels;
            fn decode_all(bytes: &[u8], cursor: std::io::Cursor<&[u8]>) {
                let _ = image::
                    load_from_memory(bytes);
                let _ = decode_pixels(bytes);
                let _ = image::codecs::png::PngDecoder::new(cursor);
                let _ = tiff::decoder::Decoder::new(cursor);
                let _ = sceptre::Image::from_bytes(bytes);
                let _ = lib.decode(&handle, color_space, None);
            }
        "#;

        let calls = direct_decoder_calls(source);

        assert_eq!(calls.len(), 6, "every direct decoder family must be denied");
    }

    #[test]
    fn source_audit_ignores_comments_and_string_literals() {
        let source = r#"
            fn messages() {
                // image::load_from_memory(bytes)
                let message = "tiff::decoder::Decoder::new(cursor)";
            }
        "#;

        assert!(direct_decoder_calls(source).is_empty());
    }

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

    type UseAliases = BTreeMap<String, Vec<String>>;

    fn collect_use_aliases(tree: &syn::UseTree, prefix: &mut Vec<String>, aliases: &mut UseAliases) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                collect_use_aliases(&path.tree, prefix, aliases);
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                let mut full_path = prefix.clone();
                full_path.push(name.ident.to_string());
                aliases
                    .entry(name.ident.to_string())
                    .or_default()
                    .push(full_path.join("::"));
            }
            syn::UseTree::Rename(rename) => {
                let mut full_path = prefix.clone();
                full_path.push(rename.ident.to_string());
                aliases
                    .entry(rename.rename.to_string())
                    .or_default()
                    .push(full_path.join("::"));
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    collect_use_aliases(item, prefix, aliases);
                }
            }
            syn::UseTree::Glob(_) => {}
        }
    }

    fn resolved_paths(path: &syn::Path, aliases: &UseAliases) -> Vec<String> {
        let mut segments = path.segments.iter().map(|segment| segment.ident.to_string());
        let Some(first) = segments.next() else {
            return Vec::new();
        };
        let suffix: Vec<_> = segments.collect();
        let Some(prefixes) = aliases.get(&first) else {
            return vec![std::iter::once(first).chain(suffix).collect::<Vec<_>>().join("::")];
        };
        prefixes
            .iter()
            .map(|prefix| {
                std::iter::once(prefix.clone())
                    .chain(suffix.clone())
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .collect()
    }

    fn is_direct_decoder_path(path: &str) -> bool {
        matches!(
            path,
            "image::open"
                | "image::load_from_memory"
                | "image::load_from_memory_with_format"
                | "image::ImageReader::new"
                | "image::ImageReader::with_format"
                | "hayro_jbig2::Image::new"
                | "hayro_jpeg2000::Image::new"
                | "tiff::decoder::Decoder::new"
                | "sceptre::Image::from_bytes"
                | "xberg_libheif::HeifContext::read_from_bytes"
        ) || (path.starts_with("image::codecs::") && path.ends_with("Decoder::new"))
    }

    struct UseAliasVisitor {
        aliases: UseAliases,
    }

    impl<'ast> syn::visit::Visit<'ast> for UseAliasVisitor {
        fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
            if item.ident != "tests" {
                syn::visit::visit_item_mod(self, item);
            }
        }

        fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
            collect_use_aliases(&item.tree, &mut Vec::new(), &mut self.aliases);
        }
    }

    struct DecoderCallVisitor<'a> {
        aliases: &'a UseAliases,
        calls: usize,
    }

    impl<'ast> syn::visit::Visit<'ast> for DecoderCallVisitor<'_> {
        fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
            if item.ident != "tests" {
                syn::visit::visit_item_mod(self, item);
            }
        }

        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            if let syn::Expr::Path(function) = call.func.as_ref()
                && resolved_paths(&function.path, self.aliases)
                    .iter()
                    .any(|path| is_direct_decoder_path(path))
            {
                self.calls += 1;
            }
            syn::visit::visit_expr_call(self, call);
        }

        fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
            let decodes_heif_handle = call.method == "decode"
                && call.args.first().is_some_and(|argument| {
                    matches!(argument, syn::Expr::Reference(reference) if matches!(reference.expr.as_ref(), syn::Expr::Path(path) if path.path.is_ident("handle")))
                });
            self.calls += usize::from(decodes_heif_handle);
            syn::visit::visit_expr_method_call(self, call);
        }
    }

    fn direct_decoder_calls(source: &str) -> Vec<usize> {
        use syn::visit::Visit;

        let syntax = syn::parse_file(source).expect("Rust source must parse");
        let mut alias_visitor = UseAliasVisitor {
            aliases: BTreeMap::new(),
        };
        alias_visitor.visit_file(&syntax);
        let mut visitor = DecoderCallVisitor {
            aliases: &alias_visitor.aliases,
            calls: 0,
        };
        visitor.visit_file(&syntax);
        (0..visitor.calls).collect()
    }

    #[test]
    fn direct_image_decode_api_calls_are_audited() {
        let wrapper_allowlist = [
            "core/image_encode.rs",
            "extraction/heif.rs",
            "extraction/image.rs",
            "extraction/image_decode.rs",
            "ocr/tesseract_wasm_backend.rs",
        ];
        let test_only_sources = ["paddle_ocr/tract_parity.rs"];
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_sources(&source_root, &mut files);
        let mut violations = BTreeMap::new();
        for path in files {
            let relative = path
                .strip_prefix(&source_root)
                .expect("source path under root")
                .to_string_lossy()
                .replace('\\', "/");
            if wrapper_allowlist.contains(&relative.as_str()) || test_only_sources.contains(&relative.as_str()) {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("read Rust source");
            let count = direct_decoder_calls(&source).len();
            if count > 0 {
                violations.insert(relative, count);
            }
        }
        assert_eq!(
            violations,
            BTreeMap::new(),
            "direct image decoder calls must live in an audited wrapper"
        );
    }
}
