---
id: fixture_dart_error_extract_input_conflicting_ocr
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

extract force+disable OCR

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final _input = await createExtractInputFromJson(json: '{"bytes":"test_documents/text/fake_text.txt","config":{"disable_ocr":true,"force_ocr":true},"filename":"fake_text.txt","kind":"bytes","mime_type":"text/plain"}');
  final _config = await createExtractionConfigFromJson(json: '{"disable_ocr":true,"force_ocr":true}');
  final result = await XbergBridge.extract(_input, config: _config);
}

```
