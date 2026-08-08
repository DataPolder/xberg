```swift title="Swift"
import Foundation
import Xberg
import RustBridge

// Trait bridges cross the FFI as JSON strings: `extract` receives a serialized
// `ExtractInput` plus `ExtractionConfig` and returns a serialized
// `ExtractedDocument`.
final class JsonExtractor: SwiftDocumentExtractorBridge {
    var name: String { "custom-json-extractor" }

    func version() -> String { "1.0.0" }

    func initialize() throws {}

    func shutdown() throws {}

    func supportedMimeTypes() -> [String] { ["application/json", "text/json"] }

    // Priority 60 outranks the built-in extractors (50) for those MIME types.
    func priority() -> Int32 { 60 }

    func canHandle(path: URL, mimeType: String) -> Bool {
        mimeType == "application/json" || path.pathExtension == "json"
    }

    func extract(input: String, config: String) throws -> String {
        struct ExtractorError: LocalizedError {
            let errorDescription: String?
        }

        guard let inputData = input.data(using: .utf8),
              let decodedInput = try JSONSerialization.jsonObject(with: inputData) as? [String: Any]
        else {
            throw ExtractorError(errorDescription: "custom-json-extractor: malformed input JSON")
        }

        // `kind: "uri"` inputs carry a path; `kind: "bytes"` inputs carry the payload.
        let payload: Data
        if let uri = decodedInput["uri"] as? String {
            payload = try Data(contentsOf: URL(fileURLWithPath: uri))
        } else if let bytes = decodedInput["bytes"] as? [UInt8] {
            payload = Data(bytes)
        } else {
            throw ExtractorError(errorDescription: "custom-json-extractor: input has neither bytes nor uri")
        }

        let document = try JSONSerialization.jsonObject(with: payload)
        let result: [String: Any] = [
            "content": Self.flatten(document),
            "mime_type": "application/json",
        ]
        let encoded = try JSONSerialization.data(withJSONObject: result)
        return String(decoding: encoded, as: UTF8.self)
    }

    /// Flattens every string leaf of a JSON value into plain text.
    private static func flatten(_ value: Any) -> String {
        if let text = value as? String {
            return text + "\n"
        }
        if let array = value as? [Any] {
            return array.map(flatten).joined()
        }
        if let object = value as? [String: Any] {
            return object.values.map(flatten).joined()
        }
        return ""
    }
}

try Xberg.registerDocumentExtractor(JsonExtractor())

// Remove it again when the plugin is no longer needed.
try Xberg.unregisterDocumentExtractor(name: "custom-json-extractor")
```
