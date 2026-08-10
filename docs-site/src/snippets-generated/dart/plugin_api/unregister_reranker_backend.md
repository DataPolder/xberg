---
id: fixture_dart_unregister_reranker_backend
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

unregister_reranker_backend

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.unregisterRerankerBackend('test-reranker-backend');
}

```
