---
id: fixture_kotlin_android_config_security_limits
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Tests archive extraction with custom security limits

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
