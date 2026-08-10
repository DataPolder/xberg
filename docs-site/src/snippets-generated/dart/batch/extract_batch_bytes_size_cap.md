---
id: fixture_dart_extract_batch_bytes_size_cap
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

extract_batch: archive size cap triggers error

```dart title="Dart"
import 'dart:convert';
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final inputs = await Future.wait((jsonDecode(r'[{"bytes":"test_documents/text/fake_text.txt","kind":"bytes","mime_type":"text/plain"}]') as List<dynamic>).cast<Map<String, dynamic>>().map((m) => createExtractInputFromJson(json: jsonEncode(m))));
  final _config = await createExtractionConfigFromJson(json: '{"security_limits":{"max_content_size":1}}');
  final result = await XbergBridge.extractBatch(inputs, config: _config);
}

```
