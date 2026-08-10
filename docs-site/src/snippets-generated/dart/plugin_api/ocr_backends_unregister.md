---
id: fixture_dart_ocr_backends_unregister
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

Unregister nonexistent OCR backend gracefully

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.unregisterOcrBackend('nonexistent-backend-xyz');
}

```
