---
id: fixture_kotlin_android_api_extract_uri
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Tests URI extraction API

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, ExtractionConfig())
}

```
