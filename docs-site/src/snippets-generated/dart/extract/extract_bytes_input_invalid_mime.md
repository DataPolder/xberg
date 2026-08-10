---
id: fixture_dart_extract_bytes_input_invalid_mime
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

extract bytes input with unsupported MIME type

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final _input = await createExtractInputFromJson(json: '{"bytes":"test_documents/text/plain.txt","config":{},"filename":"plain.txt","kind":"bytes","mime_type":"application/x-nonexistent"}');
  final _config = await createExtractionConfigFromJson(json: '{}');
  final result = await XbergBridge.extract(_input, config: _config);
}

```
