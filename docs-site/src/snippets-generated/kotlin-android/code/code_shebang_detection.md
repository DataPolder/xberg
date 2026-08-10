---
id: fixture_kotlin_android_code_shebang_detection
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Test language detection from shebang line via bytes input

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, ExtractionConfig())
}

```
