---
id: fixture_dart_config_llm_structured_extraction
language: dart
target: dart
level: typecheck
requires: []
side_effect: server
---

Tests structured extraction via liter-llm with JSON schema

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final _input = await createExtractInputFromJson(json: '{"kind":"uri","uri":"https://example.com/pdf/fake_memo.pdf"}');
  final _config = await createExtractionConfigFromJson(json: '{"structured_extraction":{"llm":{"model":"openai/gpt-4o"},"schema":{"properties":{"date":{"type":"string"},"summary":{"type":"string"},"title":{"type":"string"}},"required":["title"],"type":"object"},"schema_name":"memo_data"}}');
  final result = await XbergBridge.extract(_input, config: _config);
}

```
