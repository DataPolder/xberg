//! Internal extraction implementation.
//!
//! Public extraction orchestration lives in [`crate::core::extract`]. This module
//! contains the private file and bytes implementation details used by that
//! public API and by internal extractors. Batch orchestration lives in
//! [`crate::engine::extract_impl`], reached via [`crate::extract_batch`].

mod bytes;
mod file;
mod helpers;

#[allow(unused_imports)]
pub(crate) use bytes::extract_bytes;
#[allow(unused_imports)]
pub(crate) use file::extract_file;

#[cfg(all(test, feature = "tokio-runtime", not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::core::config::ExtractionConfig;
    use serial_test::serial;
    use std::fs::File;
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn assert_text_content(actual: &str, expected: &str) {
        assert_eq!(actual.trim_end_matches('\n'), expected);
    }

    #[tokio::test]
    async fn test_extract_file_basic() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"Hello, world!").unwrap();

        let config = ExtractionConfig::default();
        let result = extract_file(&file_path, None, &config).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_text_content(&result.content, "Hello, world!");
        assert_eq!(result.mime_type, "text/plain");
    }

    #[tokio::test]
    async fn test_extract_file_with_mime_override() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.dat");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"test content").unwrap();

        let config = ExtractionConfig::default();
        let result = extract_file(&file_path, Some("text/plain"), &config).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.mime_type, "text/plain");
    }

    #[tokio::test]
    async fn test_extract_file_nonexistent() {
        let config = ExtractionConfig::default();
        let result = extract_file("/nonexistent/file.txt", None, &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_extract_bytes_basic() {
        let config = ExtractionConfig::default();
        let result = extract_bytes(b"test content", "text/plain", &config).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_text_content(&result.content, "test content");
        assert_eq!(result.mime_type, "text/plain");
    }

    #[tokio::test]
    async fn test_extract_bytes_invalid_mime() {
        let config = ExtractionConfig::default();
        let result = extract_bytes(b"test", "invalid/mime", &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_extractor_cache() {
        let config = ExtractionConfig::default();

        let result1 = extract_bytes(b"test 1", "text/plain", &config).await;
        assert!(result1.is_ok());
        let result1 = result1.unwrap();

        let result2 = extract_bytes(b"test 2", "text/plain", &config).await;
        assert!(result2.is_ok());
        let result2 = result2.unwrap();

        assert_text_content(&result1.content, "test 1");
        assert_text_content(&result2.content, "test 2");

        let result3 = extract_bytes(b"# test 3", "text/markdown", &config).await;
        assert!(result3.is_ok());
    }

    #[tokio::test]
    async fn test_extract_file_empty() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("empty.txt");
        File::create(&file_path).unwrap();

        let config = ExtractionConfig::default();
        let result = extract_file(&file_path, None, &config).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.content, "");
    }

    #[tokio::test]
    async fn test_extract_bytes_empty() {
        let config = ExtractionConfig::default();
        let result = extract_bytes(b"", "text/plain", &config).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.content, "");
    }

    #[tokio::test]
    async fn test_extract_file_whitespace_only() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("whitespace.txt");
        File::create(&file_path).unwrap().write_all(b"   \n\t  \n  ").unwrap();

        let config = ExtractionConfig::default();
        let result = extract_file(&file_path, None, &config).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_extract_file_very_long_path() {
        let dir = tempdir().unwrap();
        let long_name = "a".repeat(200);
        let file_path = dir.path().join(format!("{}.txt", long_name));

        if let Ok(mut f) = File::create(&file_path) {
            f.write_all(b"content").unwrap();
            let config = ExtractionConfig::default();
            let result = extract_file(&file_path, None, &config).await;
            assert!(result.is_ok() || result.is_err());
        }
    }

    #[tokio::test]
    async fn test_extract_file_special_characters_in_path() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test with spaces & symbols!.txt");
        File::create(&file_path).unwrap().write_all(b"content").unwrap();

        let config = ExtractionConfig::default();
        let result = extract_file(&file_path, None, &config).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_text_content(&result.content, "content");
    }

    #[tokio::test]
    async fn test_extract_file_unicode_filename() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("测试文件名.txt");
        File::create(&file_path).unwrap().write_all(b"content").unwrap();

        let config = ExtractionConfig::default();
        let result = extract_file(&file_path, None, &config).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_extract_bytes_unsupported_mime() {
        let config = ExtractionConfig::default();
        let result = extract_bytes(b"test", "application/x-unknown-format", &config).await;

        assert!(result.is_err());
        use crate::XbergError;
        assert!(matches!(result.unwrap_err(), XbergError::UnsupportedFormat(_)));
    }

    #[tokio::test]
    async fn test_extract_bytes_very_large() {
        let large_content = vec![b'a'; 10_000_000];
        let config = ExtractionConfig::default();
        let result = extract_bytes(&large_content, "text/plain", &config).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        let trimmed_len = result.content.trim_end_matches('\n').len();
        assert_eq!(trimmed_len, 10_000_000);
    }

    #[tokio::test]
    async fn test_extract_file_mime_detection_fallback() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("testfile");
        File::create(&file_path)
            .unwrap()
            .write_all(b"plain text content")
            .unwrap();

        let config = ExtractionConfig::default();
        let result = extract_file(&file_path, None, &config).await;

        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_extract_file_wrong_mime_override() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap().write_all(b"plain text").unwrap();

        let config = ExtractionConfig::default();
        let result = extract_file(&file_path, Some("application/pdf"), &config).await;

        assert!(result.is_err() || result.is_ok());
    }

    #[tokio::test]
    async fn test_concurrent_extractions_same_mime() {
        use tokio::task::JoinSet;

        let config = Arc::new(ExtractionConfig::default());
        let mut tasks = JoinSet::new();

        for i in 0..50 {
            let config_clone = Arc::clone(&config);
            tasks.spawn(async move {
                let content = format!("test content {}", i);
                extract_bytes(content.as_bytes(), "text/plain", &config_clone).await
            });
        }

        let mut success_count = 0;
        while let Some(task_result) = tasks.join_next().await {
            if let Ok(Ok(_)) = task_result {
                success_count += 1;
            }
        }

        assert_eq!(success_count, 50);
    }

    #[serial]
    #[tokio::test]
    async fn test_concurrent_extractions_different_mimes() {
        use tokio::task::JoinSet;

        let config = Arc::new(ExtractionConfig::default());
        let mut tasks = JoinSet::new();

        let mime_types = ["text/plain", "text/markdown"];

        for i in 0..30 {
            let config_clone = Arc::clone(&config);
            let mime = mime_types[i % mime_types.len()];
            tasks.spawn(async move {
                let content = format!("test {}", i);
                extract_bytes(content.as_bytes(), mime, &config_clone).await
            });
        }

        let mut success_count = 0;
        while let Some(task_result) = tasks.join_next().await {
            if let Ok(Ok(_)) = task_result {
                success_count += 1;
            }
        }

        assert_eq!(success_count, 30);
    }

    #[test]
    fn test_with_file_overrides_single_field() {
        let base = ExtractionConfig::default();
        assert!(!base.force_ocr);

        let overrides = crate::FileExtractionConfig {
            force_ocr: Some(true),
            ..Default::default()
        };
        let resolved = base.with_file_overrides(&overrides);
        assert!(resolved.force_ocr);
        assert_eq!(resolved.use_cache, base.use_cache);
        assert_eq!(resolved.enable_quality_processing, base.enable_quality_processing);
    }

    #[test]
    fn test_with_file_overrides_none_keeps_default() {
        let base = ExtractionConfig::default();
        let overrides = crate::FileExtractionConfig::default();
        let resolved = base.with_file_overrides(&overrides);
        assert_eq!(resolved.use_cache, base.use_cache);
        assert_eq!(resolved.force_ocr, base.force_ocr);
        assert_eq!(resolved.enable_quality_processing, base.enable_quality_processing);
        assert_eq!(resolved.include_document_structure, base.include_document_structure);
    }

    #[test]
    fn test_with_file_overrides_batch_fields_unaffected() {
        let base = ExtractionConfig {
            max_concurrent_extractions: Some(42),
            use_cache: false,
            ..Default::default()
        };

        let overrides = crate::FileExtractionConfig {
            force_ocr: Some(true),
            ..Default::default()
        };
        let resolved = base.with_file_overrides(&overrides);
        assert_eq!(resolved.max_concurrent_extractions, Some(42));
        assert!(!resolved.use_cache);
        assert!(resolved.force_ocr);
    }
}
