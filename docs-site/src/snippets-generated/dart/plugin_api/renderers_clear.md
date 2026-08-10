---
id: fixture_dart_renderers_clear
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

Clear all renderers and verify list is empty

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.clearRenderers();
}

```
