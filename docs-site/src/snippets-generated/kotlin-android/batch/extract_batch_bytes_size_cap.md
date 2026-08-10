---
id: fixture_kotlin_android_extract_batch_bytes_size_cap
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: safe
---

extract_batch: archive size cap triggers error

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extractBatch(listOf(MAPPER.readValue("{\"bytes\":\"test_documents/text/fake_text.txt\",\"kind\":\"bytes\",\"mime_type\":\"text/plain\"}", ExtractInput::class.java)), config)
}

```
