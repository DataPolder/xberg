#![cfg(feature = "html")]

use xberg::ConversionOptions;
use xberg::extraction::html::convert_html_to_markdown;

#[test]
fn html_input_size_limit_is_available_through_xberg_options() {
    let options = ConversionOptions {
        max_input_size: Some(8),
        ..ConversionOptions::default()
    };

    let error = convert_html_to_markdown("<p>too long</p>", Some(options), None)
        .expect_err("HTML larger than max_input_size must be rejected");
    assert_eq!(
        error.to_string(),
        "Parsing error: HTML conversion failed: Input size 15 bytes exceeds the configured maximum of 8 bytes"
    );
}
