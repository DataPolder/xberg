---
id: fixture_dart_format_hwpx_standalone
language: dart
target: dart
level: typecheck
requires: []
side_effect: server
---

Standalone HWPX extraction using extract

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final _input = await createExtractInputFromJson(json: '{"filename":"simple.hwpx","kind":"uri","mime_type":"application/haansofthwpx","uri":"https://example.com/hwpx/simple.hwpx"}');
  final _config = await createExtractionConfigFromJson(json: '{}');
  final result = await XbergBridge.extract(_input, config: _config);
}

```
