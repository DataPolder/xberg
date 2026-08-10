---
id: fixture_kotlin_android_url_html_page_extract
language: kotlin
target: kotlin_android
level: typecheck
requires: []
side_effect: server
---

extract: website URL returns page content

```kotlin title="Kotlin (Android)"
import io.xberg.*

fun main() = kotlinx.coroutines.runBlocking {
    val result = Xberg.extract(input, config)
}

```
