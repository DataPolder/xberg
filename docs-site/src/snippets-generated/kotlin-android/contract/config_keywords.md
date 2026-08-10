---
id: fixture_kotlin_android_config_keywords
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Tests keyword extraction via YAKE algorithm

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
