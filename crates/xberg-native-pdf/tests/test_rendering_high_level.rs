mod tests {
    use xberg_native_pdf::api::Pdf;
    use xberg_native_pdf::rendering::{ImageFormat, RenderOptions};

    #[test]
    fn test_render_page_high_level_api() {
        let mut pdf = Pdf::from_text("Hello World").unwrap();

        let options = RenderOptions::default();
        let image = pdf.render_page(0, Some(&options)).unwrap();

        assert!(image.width > 0);
        assert!(image.height > 0);
        assert_eq!(image.format, ImageFormat::Png);
        assert!(!image.data.is_empty());
        assert!(image.data.starts_with(b"\x89PNG"));
    }

    #[test]
    fn test_render_page_jpeg_format() {
        let mut pdf = Pdf::from_text("Hello JPEG").unwrap();

        let options = RenderOptions::with_dpi(72).as_jpeg(80);
        let image = pdf.render_page(0, Some(&options)).unwrap();

        assert_eq!(image.format, ImageFormat::Jpeg);
        assert!(!image.data.is_empty());
        assert_eq!(image.data[0], 0xFF);
        assert_eq!(image.data[1], 0xD8);
    }
}
