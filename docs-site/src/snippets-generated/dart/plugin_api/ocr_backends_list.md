---
id: fixture_dart_ocr_backends_list
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

List all registered OCR backends

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.listOcrBackends();
}

```
