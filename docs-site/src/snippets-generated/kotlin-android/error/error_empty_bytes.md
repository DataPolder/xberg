---
id: fixture_kotlin_android_error_empty_bytes
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: safe
---

Graceful handling of empty bytes (should not error)

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
