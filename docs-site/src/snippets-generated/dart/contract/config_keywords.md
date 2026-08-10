---
id: fixture_dart_config_keywords
language: dart
target: dart
level: typecheck
requires: []
side_effect: server
---

Tests keyword extraction via YAKE algorithm

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final _input = await createExtractInputFromJson(json: '{"kind":"uri","uri":"https://example.com/pdf/fake_memo.pdf"}');
  final _config = await createExtractionConfigFromJson(json: '{"keywords":{"algorithm":"yake","max_keywords":10}}');
  final result = await XbergBridge.extract(_input, config: _config);
}

```
