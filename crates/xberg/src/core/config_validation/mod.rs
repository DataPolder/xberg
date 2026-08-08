//! Configuration validation module.
//!
//! Provides centralized validation for configuration values across all bindings.
//! This eliminates duplication of validation logic in Python, TypeScript, Java, Go, and other language bindings.
//!
//! All validation functions return `Result<()>` and produce detailed error messages
//! suitable for user-facing error handling.
//!
//! # Examples
//!
//! ```ignore
//! use xberg::core::config_validation::{
//!     validate_binarization_method,
//!     validate_token_reduction_level,
//!     validate_language_code,
//! };
//!
//! // Valid values
//! assert!(validate_binarization_method("otsu").is_ok());
//! assert!(validate_token_reduction_level("moderate").is_ok());
//! assert!(validate_language_code("en").is_ok());
//!
//! // Invalid values
//! assert!(validate_binarization_method("invalid").is_err());
//! assert!(validate_token_reduction_level("extreme").is_err());
//! ```

mod dependencies;
mod sections;

pub(crate) use dependencies::{validate_cors_origin, validate_host, validate_port, validate_upload_size};
pub(crate) use sections::{
    validate_chunking_params, validate_confidence, validate_csv_delimiter, validate_dpi, validate_language_code,
    validate_ocr_backend, validate_token_reduction_level, validate_vlm_backend_config,
};

pub(crate) use sections::{validate_binarization_method, validate_tesseract_oem, validate_tesseract_psm};

// `validate_output_format` stays `#[cfg(test)]`-only, and correctly so: both
// `ExtractionConfig::output_format` and `OcrConfig::output_format` are the strongly-typed
// `OutputFormat` enum, which rejects invalid values at deserialization time. There is no
// raw-string field left for this function to validate.
#[cfg(test)]
pub(crate) use sections::validate_output_format;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_binarization_method_valid() {
        assert!(validate_binarization_method("otsu").is_ok());
        assert!(validate_binarization_method("adaptive").is_ok());
        assert!(validate_binarization_method("sauvola").is_ok());
    }

    #[test]
    fn test_validate_binarization_method_case_insensitive() {
        assert!(validate_binarization_method("OTSU").is_ok());
        assert!(validate_binarization_method("Adaptive").is_ok());
        assert!(validate_binarization_method("SAUVOLA").is_ok());
    }

    #[test]
    fn test_validate_binarization_method_invalid() {
        let result = validate_binarization_method("invalid");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Invalid binarization method"));
        assert!(msg.contains("otsu"));
    }

    #[test]
    fn test_validate_token_reduction_level_valid() {
        assert!(validate_token_reduction_level("off").is_ok());
        assert!(validate_token_reduction_level("light").is_ok());
        assert!(validate_token_reduction_level("moderate").is_ok());
        assert!(validate_token_reduction_level("aggressive").is_ok());
        assert!(validate_token_reduction_level("maximum").is_ok());
    }

    #[test]
    fn test_validate_token_reduction_level_case_insensitive() {
        assert!(validate_token_reduction_level("OFF").is_ok());
        assert!(validate_token_reduction_level("Moderate").is_ok());
        assert!(validate_token_reduction_level("MAXIMUM").is_ok());
    }

    #[test]
    fn test_validate_token_reduction_level_invalid() {
        let result = validate_token_reduction_level("extreme");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Invalid token reduction level"));
    }

    #[test]
    fn test_validate_ocr_backend_valid() {
        assert!(validate_ocr_backend("tesseract").is_ok());
        assert!(validate_ocr_backend("paddleocr").is_ok());
        assert!(validate_ocr_backend("sceptre").is_ok());
    }

    #[test]
    fn test_validate_ocr_backend_case_insensitive() {
        assert!(validate_ocr_backend("TESSERACT").is_ok());
        assert!(validate_ocr_backend("PADDLEOCR").is_ok());
        assert!(validate_ocr_backend("SCEPTRE").is_ok());
    }

    #[test]
    fn test_validate_ocr_backend_rejects_unknown_backend() {
        let result = validate_ocr_backend("unsupported-ocr");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Invalid OCR backend"));
    }

    #[test]
    fn test_validate_ocr_backend_invalid() {
        let result = validate_ocr_backend("invalid_backend");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Invalid OCR backend"));
    }

    #[test]
    fn test_validate_language_code_valid_iso639_1() {
        assert!(validate_language_code("en").is_ok());
        assert!(validate_language_code("de").is_ok());
        assert!(validate_language_code("fr").is_ok());
        assert!(validate_language_code("es").is_ok());
        assert!(validate_language_code("zh").is_ok());
        assert!(validate_language_code("ja").is_ok());
        assert!(validate_language_code("ko").is_ok());
    }

    #[test]
    fn test_validate_language_code_valid_iso639_3() {
        assert!(validate_language_code("eng").is_ok());
        assert!(validate_language_code("deu").is_ok());
        assert!(validate_language_code("fra").is_ok());
        assert!(validate_language_code("spa").is_ok());
        assert!(validate_language_code("zho").is_ok());
        assert!(validate_language_code("jpn").is_ok());
        assert!(validate_language_code("jpn_vert").is_ok());
        assert!(validate_language_code("JPN_VERT").is_ok());
        assert!(validate_language_code("kor").is_ok());
        for language in ["afr", "aze", "bos", "bel", "kaz", "kir", "srp", "tgk"] {
            assert!(
                validate_language_code(language).is_ok(),
                "Sceptre language {language} should pass shared validation"
            );
        }
        for language in ["ch_sim", "rs_latin", "rs-cyrillic", "tel", "kan", "abq", "tjk"] {
            assert!(
                validate_language_code(language).is_ok(),
                "EasyOCR Gen2 language {language} should pass shared validation"
            );
        }
    }

    #[test]
    fn test_validate_language_code_case_insensitive() {
        assert!(validate_language_code("EN").is_ok());
        assert!(validate_language_code("ENG").is_ok());
        assert!(validate_language_code("De").is_ok());
        assert!(validate_language_code("DEU").is_ok());
    }

    #[test]
    fn test_validate_language_code_all_keyword() {
        assert!(validate_language_code("all").is_ok());
        assert!(validate_language_code("ALL").is_ok());
        assert!(validate_language_code("All").is_ok());
        assert!(validate_language_code("*").is_ok());
    }

    #[test]
    fn test_validate_language_code_invalid() {
        let result = validate_language_code("invalid");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Invalid language code"));
        assert!(msg.contains("ISO 639"));
    }

    #[test]
    fn test_validate_tesseract_psm_valid() {
        for psm in 0..=13 {
            assert!(validate_tesseract_psm(psm).is_ok(), "PSM {} should be valid", psm);
        }
    }

    #[test]
    fn test_validate_tesseract_psm_invalid() {
        assert!(validate_tesseract_psm(-1).is_err());
        assert!(validate_tesseract_psm(14).is_err());
        assert!(validate_tesseract_psm(100).is_err());
    }

    #[test]
    fn test_validate_tesseract_oem_valid() {
        for oem in 0..=3 {
            assert!(validate_tesseract_oem(oem).is_ok(), "OEM {} should be valid", oem);
        }
    }

    #[test]
    fn test_validate_tesseract_oem_invalid() {
        assert!(validate_tesseract_oem(-1).is_err());
        assert!(validate_tesseract_oem(4).is_err());
        assert!(validate_tesseract_oem(10).is_err());
    }

    #[test]
    fn test_validate_output_format_valid() {
        assert!(validate_output_format("text").is_ok());
        assert!(validate_output_format("markdown").is_ok());
    }

    #[test]
    fn test_validate_output_format_case_insensitive() {
        assert!(validate_output_format("TEXT").is_ok());
        assert!(validate_output_format("Markdown").is_ok());
    }

    #[test]
    fn test_validate_output_format_invalid() {
        let result = validate_output_format("xml");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Invalid output format"));
    }

    #[test]
    fn test_validate_confidence_valid() {
        assert!(validate_confidence(0.0).is_ok());
        assert!(validate_confidence(0.5).is_ok());
        assert!(validate_confidence(1.0).is_ok());
        assert!(validate_confidence(0.75).is_ok());
    }

    #[test]
    fn test_validate_confidence_invalid() {
        assert!(validate_confidence(-0.1).is_err());
        assert!(validate_confidence(1.1).is_err());
        assert!(validate_confidence(2.0).is_err());
    }

    #[test]
    fn test_validate_dpi_valid() {
        assert!(validate_dpi(72).is_ok());
        assert!(validate_dpi(96).is_ok());
        assert!(validate_dpi(300).is_ok());
        assert!(validate_dpi(600).is_ok());
        assert!(validate_dpi(1).is_ok());
    }

    #[test]
    fn test_validate_dpi_invalid() {
        assert!(validate_dpi(0).is_err());
        assert!(validate_dpi(-1).is_err());
        assert!(validate_dpi(2401).is_err());
    }

    #[test]
    fn test_validate_chunking_params_valid() {
        assert!(validate_chunking_params(1000, 200).is_ok());
        assert!(validate_chunking_params(500, 50).is_ok());
        assert!(validate_chunking_params(1, 0).is_ok());
    }

    #[test]
    fn test_validate_chunking_params_zero_chars() {
        let result = validate_chunking_params(0, 100);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("max_chars"));
    }

    #[test]
    fn test_validate_chunking_params_overlap_too_large() {
        let result = validate_chunking_params(100, 100);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("overlap"));

        let result = validate_chunking_params(100, 150);
        assert!(result.is_err());
    }

    #[test]
    fn test_error_messages_are_helpful() {
        let err = validate_binarization_method("bad").unwrap_err().to_string();
        assert!(err.contains("otsu"));
        assert!(err.contains("adaptive"));
        assert!(err.contains("sauvola"));

        let err = validate_token_reduction_level("bad").unwrap_err().to_string();
        assert!(err.contains("off"));
        assert!(err.contains("moderate"));

        let err = validate_language_code("bad").unwrap_err().to_string();
        assert!(err.contains("ISO 639"));
        assert!(err.contains("en"));
    }

    /// Render a validation outcome as an exactly comparable string: the empty string for
    /// an accepted value, or the full `Display` text of the rejection.
    fn outcome(result: crate::Result<()>) -> String {
        match result {
            Ok(()) => String::new(),
            Err(error) => error.to_string(),
        }
    }

    const PORT_ZERO_REJECTION: &str = "Validation error: Port must be 1-65535, got 0. \
         Set 'server.port' (or XBERG_PORT) to a free port such as 8000.";

    const EMPTY_HOST_REJECTION: &str = "Validation error: Invalid host '': \
         must be a valid IP address or hostname. Set 'server.host' (or XBERG_HOST) to e.g. \
         '127.0.0.1', '0.0.0.0' or 'localhost'.";

    const UPLOAD_SIZE_ZERO_REJECTION: &str = "Validation error: Upload size must be greater than 0, got 0. \
         Set 'server.max_request_body_bytes' / 'server.max_multipart_field_bytes' \
         to a positive byte count such as 104857600 (100 MB).";

    fn cors_rejection(origin: &str) -> String {
        format!(
            "Validation error: Invalid CORS origin '{origin}': must be a valid HTTP/HTTPS URL or '*'. \
             Set 'server.cors_origins' (or XBERG_CORS_ORIGINS) to e.g. 'https://example.com'."
        )
    }

    #[test]
    fn should_accept_port_when_inside_valid_range() {
        for port in [1_u32, 80, 443, 8000, 65535] {
            assert_eq!(outcome(validate_port(port)), "", "port {port} should be accepted");
        }
    }

    #[test]
    fn should_reject_port_when_zero() {
        assert_eq!(outcome(validate_port(0)), PORT_ZERO_REJECTION);
    }

    #[test]
    fn should_reject_port_when_above_sixteen_bit_range() {
        assert_eq!(
            outcome(validate_port(65_536)),
            "Validation error: Port must be 1-65535, got 65536. \
             Set 'server.port' (or XBERG_PORT) to a free port such as 8000."
        );
    }

    #[test]
    fn should_accept_host_when_ipv4_address() {
        for host in ["127.0.0.1", "0.0.0.0", "192.168.1.1", "10.0.0.1", "255.255.255.255"] {
            assert_eq!(outcome(validate_host(host)), "", "host {host} should be accepted");
        }
    }

    #[test]
    fn should_accept_host_when_ipv6_address() {
        for host in ["::1", "::", "2001:db8::1", "fe80::1"] {
            assert_eq!(outcome(validate_host(host)), "", "host {host} should be accepted");
        }
    }

    #[test]
    fn should_accept_host_when_dns_hostname() {
        for host in ["localhost", "example.com", "sub.example.com", "api-server", "app123"] {
            assert_eq!(outcome(validate_host(host)), "", "host {host} should be accepted");
        }
    }

    #[test]
    fn should_reject_host_when_empty() {
        assert_eq!(outcome(validate_host("")), EMPTY_HOST_REJECTION);
    }

    #[test]
    fn should_reject_host_when_it_contains_whitespace() {
        assert_eq!(
            outcome(validate_host("not a valid host")),
            "Validation error: Invalid host 'not a valid host': must be a valid IP address or hostname. \
             Set 'server.host' (or XBERG_HOST) to e.g. '127.0.0.1', '0.0.0.0' or 'localhost'."
        );
    }

    #[test]
    fn should_reject_host_when_ipv4_octets_are_out_of_range() {
        assert_eq!(
            outcome(validate_host("256.256.256.256")),
            "Validation error: Invalid host '256.256.256.256': must be a valid IP address or hostname. \
             Set 'server.host' (or XBERG_HOST) to e.g. '127.0.0.1', '0.0.0.0' or 'localhost'."
        );
    }

    #[test]
    fn should_reject_host_when_a_label_is_empty() {
        assert_eq!(
            outcome(validate_host("example..com")),
            "Validation error: Invalid host 'example..com': must be a valid IP address or hostname. \
             Set 'server.host' (or XBERG_HOST) to e.g. '127.0.0.1', '0.0.0.0' or 'localhost'."
        );
    }

    #[test]
    fn should_accept_cors_origin_when_https_url() {
        for origin in [
            "https://example.com",
            "https://localhost:3000",
            "https://sub.example.com",
            "https://192.168.1.1",
            "https://example.com/path",
        ] {
            assert_eq!(outcome(validate_cors_origin(origin)), "", "origin {origin} should pass");
        }
    }

    #[test]
    fn should_accept_cors_origin_when_http_url() {
        for origin in ["http://example.com", "http://localhost:3000", "http://127.0.0.1:8000"] {
            assert_eq!(outcome(validate_cors_origin(origin)), "", "origin {origin} should pass");
        }
    }

    #[test]
    fn should_accept_cors_origin_when_wildcard() {
        assert_eq!(outcome(validate_cors_origin("*")), "");
    }

    #[test]
    fn should_reject_cors_origin_when_scheme_is_missing() {
        assert_eq!(outcome(validate_cors_origin("not-a-url")), cors_rejection("not-a-url"));
        assert_eq!(
            outcome(validate_cors_origin("example.com")),
            cors_rejection("example.com")
        );
    }

    #[test]
    fn should_reject_cors_origin_when_scheme_is_not_http() {
        assert_eq!(
            outcome(validate_cors_origin("ftp://example.com")),
            cors_rejection("ftp://example.com")
        );
    }

    #[test]
    fn should_reject_cors_origin_when_authority_is_empty() {
        assert_eq!(outcome(validate_cors_origin("http://")), cors_rejection("http://"));
        assert_eq!(
            outcome(validate_cors_origin("https:///path")),
            cors_rejection("https:///path")
        );
    }

    #[test]
    fn should_accept_upload_size_when_positive() {
        for size in [1_usize, 1024, 1_000_000, 1_000_000_000, usize::MAX] {
            assert_eq!(
                outcome(validate_upload_size(size)),
                "",
                "size {size} should be accepted"
            );
        }
    }

    #[test]
    fn should_reject_upload_size_when_zero() {
        assert_eq!(outcome(validate_upload_size(0)), UPLOAD_SIZE_ZERO_REJECTION);
    }
}
