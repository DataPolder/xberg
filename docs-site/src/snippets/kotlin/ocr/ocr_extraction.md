```kotlin title="Kotlin"
import io.xberg.*

private const val DEFAULT_MAX_ARCHIVE_DEPTH = 3L

fun main() {
    val ocr = OcrConfig(backend = "tesseract", language = listOf("eng"))
    val config = ExtractionConfig(
        ocr = ocr,
        extractionTimeoutSecs = null,
        maxEmbeddedFileBytes = null,
        url = UrlExtractionConfig(crawl = CrawlConfig(ssrf = SsrfPolicy())),
        maxArchiveDepth = DEFAULT_MAX_ARCHIVE_DEPTH,
    )

    val resultOutput = Xberg.extract(
        ExtractInput(kind = ExtractInputKind.URI, uri = "scanned.pdf"),
        config,
    )
    val result = resultOutput.results.first()
    println(result.content)
}
```
