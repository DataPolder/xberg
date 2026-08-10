---
id: fixture_kotlin_android_config_llm_structured_extraction
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Tests structured extraction via liter-llm with JSON schema

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
