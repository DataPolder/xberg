---
id: fixture_kotlin_android_url_recursive_document_urls
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

extract: recursive URL extraction follows document links discovered in results

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
