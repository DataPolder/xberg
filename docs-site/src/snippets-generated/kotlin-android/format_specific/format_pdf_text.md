---
id: fixture_kotlin_android_format_pdf_text
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Standalone PDF text extraction using extract

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, ExtractionConfig())
}

```
