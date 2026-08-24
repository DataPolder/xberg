```rust title="Rust"
use xberg::unregister_document_extractor;

fn main() -> xberg::Result<()> {
    unregister_document_extractor("custom-json-extractor")?;
    Ok(())
}
```
