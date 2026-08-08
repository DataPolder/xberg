```swift title="Swift"
import Foundation
import Xberg
import RustBridge

/// Collects metadata from every PDF that passes through the pipeline.
///
/// `process` receives the serialized `ExtractedDocument` and returns `Void`, so
/// a Swift post-processor observes results rather than rewriting them. Use it to
/// index, audit, or report on extractions from your app.
final class PdfMetadataExtractor: SwiftPostProcessorBridge {
    struct Record {
        let title: String?
        let pageCount: Int?
        let contentLength: Int
    }

    private let lock = NSLock()
    private var records: [Record] = []

    var name: String { "pdf-metadata-extractor" }

    func version() -> String { "1.0.0" }

    func initialize() throws {}

    func shutdown() throws {}

    // Serialized `ProcessingStage`: "Early", "Middle", or "Late".
    func processingStage() -> String { "\"Late\"" }

    func priority() -> Int32 { 80 }

    func shouldProcess(result: String, config: String) -> Bool {
        guard let document = Self.decode(result) else { return false }
        return document["mime_type"] as? String == "application/pdf"
    }

    func estimatedDurationMs(result: String) -> UInt64 { 1 }

    func process(result: String, config: String) throws {
        struct ProcessorError: LocalizedError {
            let errorDescription: String?
        }

        guard let document = Self.decode(result) else {
            throw ProcessorError(errorDescription: "pdf-metadata-extractor: malformed result JSON")
        }

        let metadata = document["metadata"] as? [String: Any] ?? [:]
        let record = Record(
            title: metadata["title"] as? String,
            pageCount: metadata["page_count"] as? Int,
            contentLength: (document["content"] as? String)?.count ?? 0
        )

        lock.lock()
        records.append(record)
        lock.unlock()
    }

    /// Snapshot of everything collected so far.
    func collected() -> [Record] {
        lock.lock()
        defer { lock.unlock() }
        return records
    }

    private static func decode(_ json: String) -> [String: Any]? {
        guard let data = json.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }
}

let processor = PdfMetadataExtractor()
try Xberg.registerPostProcessor(processor)

// ... run extractions ...

for record in processor.collected() {
    print("\(record.title ?? "untitled"): \(record.contentLength) chars")
}

try Xberg.unregisterPostProcessor(name: "pdf-metadata-extractor")
```
