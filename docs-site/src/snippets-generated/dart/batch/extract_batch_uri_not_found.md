---
id: fixture_dart_extract_batch_uri_not_found
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

extract_batch with missing URI input

```dart title="Dart"
import 'dart:convert';
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final inputs = await Future.wait((jsonDecode(r'[{"kind":"uri","uri":"/nonexistent/a.pdf"}]') as List<dynamic>).cast<Map<String, dynamic>>().map((m) => createExtractInputFromJson(json: jsonEncode(m))));
  final result = await XbergBridge.extractBatch(inputs);
}

```
