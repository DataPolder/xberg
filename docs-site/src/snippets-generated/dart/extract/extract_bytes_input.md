---
id: fixture_dart_extract_bytes_input
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

extract bytes input from PDF document

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final _input = await createExtractInputFromJson(json: '{"bytes":"test_documents/pdf/fake_memo.pdf","filename":"fake_memo.pdf","kind":"bytes","mime_type":"application/pdf"}');
  final _config = await createExtractionConfigFromJson(json: '{}');
  final result = await XbergBridge.extract(_input, config: _config);
}

```
