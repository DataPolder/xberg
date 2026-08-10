---
id: fixture_kotlin_android_config_tree_sitter
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

Tests tree-sitter configuration round-trip

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
