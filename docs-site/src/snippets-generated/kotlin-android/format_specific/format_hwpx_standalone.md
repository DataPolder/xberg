---
id: fixture_kotlin_android_format_hwpx_standalone
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Standalone HWPX extraction using extract

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, ExtractionConfig())
}

```
