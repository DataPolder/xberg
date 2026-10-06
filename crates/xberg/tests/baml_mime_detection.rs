#![cfg(feature = "tree-sitter")]

use xberg::detect_mime_type;

#[test]
fn baml_extension_is_recognized_as_source_code() {
    assert_eq!(
        detect_mime_type("model.baml".to_string(), false).expect("BAML extension must be recognized"),
        "text/x-source-code"
    );
}
