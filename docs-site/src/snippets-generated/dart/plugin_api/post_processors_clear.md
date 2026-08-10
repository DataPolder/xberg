---
id: fixture_dart_post_processors_clear
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

Clear all post-processors and verify list is empty

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.clearPostProcessors();
}

```
