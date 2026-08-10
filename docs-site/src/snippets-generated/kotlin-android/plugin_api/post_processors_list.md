---
id: fixture_kotlin_android_post_processors_list
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: safe
---

List all registered post-processors

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.listPostProcessors()
}

```
