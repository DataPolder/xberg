---
id: fixture_dart_error_empty_bytes
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

Graceful handling of empty bytes (should not error)

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final _input = await createExtractInputFromJson(json: '{"bytes":[],"config":{},"filename":"empty.txt","kind":"bytes","mime_type":"text/plain"}');
  final _config = await createExtractionConfigFromJson(json: '{}');
  final result = await XbergBridge.extract(_input, config: _config);
}

```
