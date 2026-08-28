```rust title="Rust"
use xberg::plugins::{Plugin, DocumentExtractor};
use xberg::{ExtractInput, ExtractedDocument, ExtractionConfig, Result};
use async_trait::async_trait;
use tracing::{info, warn};

struct MyPlugin;

impl Plugin for MyPlugin {
    fn name(&self) -> &str {
        "my-plugin"
    }

    fn version(&self) -> String {
        "1.0.0".to_string()
    }

    fn initialize(&self) -> Result<()> {
        info!(plugin = self.name(), "initializing plugin");
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        info!(plugin = self.name(), "shutting down plugin");
        Ok(())
    }
}

#[async_trait]
impl DocumentExtractor for MyPlugin {
    async fn extract(
        &self,
        input: ExtractInput,
        _config: &ExtractionConfig,
    ) -> Result<ExtractedDocument> {
        let mime_type = input.mime_type.clone().unwrap_or_default();
        let bytes = input.bytes.unwrap_or_default();
        info!(mime_type, input_len = bytes.len(), "extracting document");

        let result = ExtractedDocument::default();

        if result.content.is_empty() {
            warn!("extraction produced empty content");
        }

        Ok(result)
    }

    fn supported_mime_types(&self) -> &[&str] {
        &["application/octet-stream"]
    }
}
```
