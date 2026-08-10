---
id: fixture_dart_format_pdf_text
language: dart
target: dart
level: typecheck
requires: []
side_effect: server
---

Standalone PDF text extraction using extract

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final _input = await createExtractInputFromJson(json: '{"filename":"fake_memo.pdf","kind":"uri","mime_type":"application/pdf","uri":"https://example.com/pdf/fake_memo.pdf"}');
  final _config = await createExtractionConfigFromJson(json: '{}');
  final result = await XbergBridge.extract(_input, config: _config);
}

```
