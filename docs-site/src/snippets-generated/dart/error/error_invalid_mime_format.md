---
id: fixture_dart_error_invalid_mime_format
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

Error when extracting with invalid MIME type format

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final _input = await createExtractInputFromJson(json: '{"bytes":"test_documents/text/plain.txt","config":{},"filename":"plain.txt","kind":"bytes","mime_type":"not-a-mime"}');
  final _config = await createExtractionConfigFromJson(json: '{}');
  final result = await XbergBridge.extract(_input, config: _config);
}

```
