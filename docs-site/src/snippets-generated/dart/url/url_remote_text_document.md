---
id: fixture_dart_url_remote_text_document
language: dart
target: dart
level: typecheck
requires: []
side_effect: server
---

extract: remote text document URL

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final _input = await createExtractInputFromJson(json: '{"kind":"uri","uri":"https://example.com"}');
  final _config = await createExtractionConfigFromJson(json: '{"url":{"mode":"document"}}');
  final result = await XbergBridge.extract(_input, config: _config);
}

```
