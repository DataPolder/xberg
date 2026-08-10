---
id: fixture_kotlin_android_config_element_types
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Tests element-based result format with element type assertions on DOCX

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
