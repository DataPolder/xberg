---
id: fixture_kotlin_android_renderers_list
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: safe
---

List all registered renderers

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.listRenderers()
}

```
