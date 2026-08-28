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
    val resultOutput = Xberg.extract(
        ExtractInput(kind = ExtractInputKind.URI, uri = "document.pdf"),
        config,
    )
    val result = resultOutput.results.first()

    val tables = result.tables ?: emptyList()
    for (table in tables) {
        println("Table on page ${table.pageNumber} with ${table.cells.size} rows")
        println(table.markdown)

        for (row in table.cells) {
            println(row)
        }
    }
}
```
