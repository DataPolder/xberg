---
id: fixture_dart_api_extract_batch_uri
language: dart
target: dart
level: typecheck
requires: []
side_effect: server
---

Tests batch URI extraction API (extract_batch)

```dart title="Dart"
import 'dart:convert';
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final inputs = await Future.wait((jsonDecode(r'[{"kind":"uri","uri":"https://example.com/pdf/fake_memo.pdf"}]') as List<dynamic>).cast<Map<String, dynamic>>().map((m) => createExtractInputFromJson(json: jsonEncode(m))));
  final result = await XbergBridge.extractBatch(inputs);
}

```
