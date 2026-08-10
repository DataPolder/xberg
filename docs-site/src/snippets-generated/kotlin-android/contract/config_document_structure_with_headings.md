---
id: fixture_kotlin_android_config_document_structure_with_headings
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Tests document structure with DOCX heading-driven nesting

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
