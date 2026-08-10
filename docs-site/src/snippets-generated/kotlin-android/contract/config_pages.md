---
id: fixture_kotlin_android_config_pages
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Tests page extraction and page marker configuration

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
