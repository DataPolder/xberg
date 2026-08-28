```kotlin title="Kotlin"
import io.xberg.*

private const val DEFAULT_MAX_ARCHIVE_DEPTH = 3L

fun main() {
    val inputs = listOf(
        ExtractInput(kind = ExtractInputKind.URI, uri = "report.pdf"),
        ExtractInput(kind = ExtractInputKind.URI, uri = "notes.txt"),
    )
    val config = ExtractionConfig(
        extractionTimeoutSecs = null,
        maxEmbeddedFileBytes = null,
        url = UrlExtractionConfig(crawl = CrawlConfig(ssrf = SsrfPolicy())),
        maxArchiveDepth = DEFAULT_MAX_ARCHIVE_DEPTH,
    )

    val output = Xberg.extractBatch(inputs, config)
    output.results.forEach { println(it.content) }
}
```
