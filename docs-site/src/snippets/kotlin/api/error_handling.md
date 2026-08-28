```kotlin title="Kotlin"
import io.xberg.*

private const val DEFAULT_MAX_ARCHIVE_DEPTH = 3L

fun main() {
    val config = ExtractionConfig(
        extractionTimeoutSecs = null,
        maxEmbeddedFileBytes = null,
        url = UrlExtractionConfig(crawl = CrawlConfig(ssrf = SsrfPolicy())),
        maxArchiveDepth = DEFAULT_MAX_ARCHIVE_DEPTH,
    )
    try {
        val resultOutput = Xberg.extract(
            ExtractInput(kind = ExtractInputKind.URI, uri = "document.pdf"),
            config,
        )
        val result = resultOutput.results.first()
        println(result.content)
    } catch (error: XbergBridgeException) {
        System.err.println("Extraction failed: ${error.message}")
    }
}
```
