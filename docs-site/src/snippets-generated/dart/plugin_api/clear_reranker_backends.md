---
id: fixture_dart_clear_reranker_backends
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

Clear all reranker backends and verify list is empty

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.clearRerankerBackends();
}

```
