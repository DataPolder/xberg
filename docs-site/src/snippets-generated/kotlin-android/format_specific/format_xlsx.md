---
id: fixture_kotlin_android_format_xlsx
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

XLSX spreadsheet extraction using extract

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, ExtractionConfig())
}

```
