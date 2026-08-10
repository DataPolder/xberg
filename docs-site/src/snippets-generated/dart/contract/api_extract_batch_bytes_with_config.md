---
id: fixture_dart_api_extract_batch_bytes_with_config
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

Tests batch bytes extraction with per-input config (extract_batch)

```dart title="Dart"
import 'dart:convert';
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final inputs = await Future.wait((jsonDecode(r'[{"bytes":"test_documents/pdf/fake_memo.pdf","config":{"output_format":"markdown"},"filename":"fake_memo.pdf","kind":"bytes"}]') as List<dynamic>).cast<Map<String, dynamic>>().map((m) => createExtractInputFromJson(json: jsonEncode(m))));
  final result = await XbergBridge.extractBatch(inputs);
}

```
