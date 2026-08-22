//! Tests for page rendering functionality.
//!
//! These tests require the "rendering" feature to be enabled.

use xberg_native_pdf::api::{ImageFormat, Pdf, RenderOptions};

mod common;

/// Helper function to create a simple test PDF.
fn create_test_pdf() -> Vec<u8> {
    let pdf = Pdf::from_text("Test document for rendering.\n\nPage 1 content.")
        .expect("Failed to create PDF");
    pdf.into_bytes()
}

mod render_options {
    use super::*;

    #[test]
    fn test_default_options() {
        let opts = RenderOptions::default();
        assert_eq!(opts.dpi, 150);
        assert_eq!(opts.format, ImageFormat::Png);
        assert!(opts.background.is_some());
        assert!(opts.render_annotations);
        assert_eq!(opts.jpeg_quality, 85);
    }

    #[test]
    fn test_with_dpi() {
        let opts = RenderOptions::with_dpi(300);
        assert_eq!(opts.dpi, 300);
        assert_eq!(opts.format, ImageFormat::Png);
    }

    #[test]
    fn test_transparent_background() {
        let opts = RenderOptions::default().with_transparent_background();
        assert!(opts.background.is_none());
    }

    #[test]
    fn test_as_jpeg() {
        let opts = RenderOptions::default().as_jpeg(90);
        assert_eq!(opts.format, ImageFormat::Jpeg);
        assert_eq!(opts.jpeg_quality, 90);
    }

    #[test]
    fn test_jpeg_quality_bounds() {
        // Quality should be clamped to 1-100 ~keep
        let opts = RenderOptions::default().as_jpeg(0);
        assert_eq!(opts.jpeg_quality, 1);

        let opts = RenderOptions::default().as_jpeg(150);
        assert_eq!(opts.jpeg_quality, 100);
    }
}

mod page_rendering {
    use super::*;

    #[test]
    fn test_render_page_basic() {
        let bytes = create_test_pdf();

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let input_path = temp_dir.path().join("test_render_input.pdf");
        std::fs::write(&input_path, &bytes).expect("Failed to write temp PDF");

        let mut pdf = Pdf::open(&input_path).expect("Failed to open PDF");

        let image = pdf.render_page(0, None).expect("Failed to render page");

        assert!(image.width > 0);
        assert!(image.height > 0);
        assert!(!image.data.is_empty());
        assert_eq!(image.format, ImageFormat::Png);

        assert_eq!(&image.data[0..4], &[0x89, 0x50, 0x4E, 0x47]);

        let _ = std::fs::remove_file(&input_path);
    }

    #[test]
    fn test_render_page_with_options() {
        let bytes = create_test_pdf();

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let input_path = temp_dir.path().join("test_render_opts_input.pdf");
        std::fs::write(&input_path, &bytes).expect("Failed to write temp PDF");

        let mut pdf = Pdf::open(&input_path).expect("Failed to open PDF");

        let options = RenderOptions::with_dpi(72);
        let image = pdf
            .render_page_with_options(0, &options)
            .expect("Failed to render page");

        assert!(image.width > 0);
        assert!(image.height > 0);

        let _ = std::fs::remove_file(&input_path);
    }

    #[test]
    fn test_render_page_to_file_png() {
        let bytes = create_test_pdf();

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let input_path = temp_dir.path().join("test_render_file_input.pdf");
        let output_path = temp_dir.path().join("test_render_output.png");
        std::fs::write(&input_path, &bytes).expect("Failed to write temp PDF");

        let mut pdf = Pdf::open(&input_path).expect("Failed to open PDF");

        pdf.render_page_to_file(0, &output_path)
            .expect("Failed to render page to file");

        assert!(output_path.exists());
        let file_bytes = std::fs::read(&output_path).expect("Failed to read output file");
        assert!(!file_bytes.is_empty());
        assert_eq!(&file_bytes[0..4], &[0x89, 0x50, 0x4E, 0x47]);

        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn test_render_page_to_file_jpeg() {
        let bytes = create_test_pdf();

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let input_path = temp_dir.path().join("test_render_jpeg_input.pdf");
        let output_path = temp_dir.path().join("test_render_output.jpg");
        std::fs::write(&input_path, &bytes).expect("Failed to write temp PDF");

        let mut pdf = Pdf::open(&input_path).expect("Failed to open PDF");

        pdf.render_page_to_file(0, &output_path)
            .expect("Failed to render page to file");

        assert!(output_path.exists());
        let file_bytes = std::fs::read(&output_path).expect("Failed to read output file");
        assert!(!file_bytes.is_empty());
        assert_eq!(&file_bytes[0..2], &[0xFF, 0xD8]);

        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn test_render_page_with_custom_dpi() {
        let bytes = create_test_pdf();

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let input_path = temp_dir.path().join("test_render_dpi_input.pdf");
        let output_path = temp_dir.path().join("test_render_dpi_output.png");
        std::fs::write(&input_path, &bytes).expect("Failed to write temp PDF");

        let mut pdf = Pdf::open(&input_path).expect("Failed to open PDF");

        pdf.render_page_to_file_with_dpi(0, &output_path, 300)
            .expect("Failed to render page to file");

        assert!(output_path.exists());

        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn test_rendered_image_save() {
        let bytes = create_test_pdf();

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let input_path = temp_dir.path().join("test_image_save_input.pdf");
        let output_path = temp_dir.path().join("test_image_save_output.png");
        std::fs::write(&input_path, &bytes).expect("Failed to write temp PDF");

        let mut pdf = Pdf::open(&input_path).expect("Failed to open PDF");

        let image = pdf.render_page(0, None).expect("Failed to render page");

        image.save(&output_path).expect("Failed to save image");

        assert!(output_path.exists());

        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn test_rendered_image_as_bytes() {
        let bytes = create_test_pdf();

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let input_path = temp_dir.path().join("test_image_bytes_input.pdf");
        std::fs::write(&input_path, &bytes).expect("Failed to write temp PDF");

        let mut pdf = Pdf::open(&input_path).expect("Failed to open PDF");

        let image = pdf.render_page(0, None).expect("Failed to render page");

        let data = image.as_bytes();
        assert!(!data.is_empty());
        assert_eq!(data, &image.data);

        let _ = std::fs::remove_file(&input_path);
    }
}

mod error_handling {
    use super::*;

    #[test]
    fn test_render_invalid_page() {
        let bytes = create_test_pdf();

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let input_path = temp_dir.path().join("test_render_invalid_input.pdf");
        std::fs::write(&input_path, &bytes).expect("Failed to write temp PDF");

        let mut pdf = Pdf::open(&input_path).expect("Failed to open PDF");

        let result = pdf.render_page(999, None);
        assert!(result.is_err());

        let _ = std::fs::remove_file(&input_path);
    }
}

mod raw_rgba {
    use super::*;

    #[test]
    fn test_raw_rgba_format_flag() {
        let opts = RenderOptions::with_dpi(72).as_raw();
        assert_eq!(opts.format, ImageFormat::RawRgba8);
    }

    #[test]
    fn test_raw_rgba_size_is_width_times_height_times_4() {
        let bytes = create_test_pdf();
        let (_dir, temp) = common::write_temp_pdf(&bytes, "test_raw_rgba_size.pdf");
        let mut pdf = Pdf::open(&temp).unwrap();

        let opts = RenderOptions::with_dpi(72).as_raw();
        let img = pdf.render_page_with_options(0, &opts).unwrap();

        assert_eq!(img.format, ImageFormat::RawRgba8);
        assert!(img.width > 0);
        assert!(img.height > 0);
        assert_eq!(img.data.len(), (img.width * img.height * 4) as usize);
    }

    #[test]
    fn test_raw_rgba_not_png_magic() {
        let bytes = create_test_pdf();
        let (_dir, temp) = common::write_temp_pdf(&bytes, "test_raw_rgba_magic.pdf");
        let mut pdf = Pdf::open(&temp).unwrap();

        let opts = RenderOptions::with_dpi(72).as_raw();
        let img = pdf.render_page_with_options(0, &opts).unwrap();

        assert_ne!(&img.data[..4], b"\x89PNG");
    }

    #[test]
    fn test_raw_rgba_dimensions_match_png() {
        let bytes = create_test_pdf();
        let (_dir, temp) = common::write_temp_pdf(&bytes, "test_raw_rgba_dims.pdf");
        let mut pdf = Pdf::open(&temp).unwrap();

        let png_img = pdf
            .render_page_with_options(0, &RenderOptions::with_dpi(72))
            .unwrap();
        let raw_img = pdf
            .render_page_with_options(0, &RenderOptions::with_dpi(72).as_raw())
            .unwrap();

        assert_eq!(png_img.width, raw_img.width);
        assert_eq!(png_img.height, raw_img.height);
    }
}
