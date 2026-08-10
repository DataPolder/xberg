---
id: fixture_kotlin_android_url_crawl_linked_pages
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

extract: crawl mode follows linked pages

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
