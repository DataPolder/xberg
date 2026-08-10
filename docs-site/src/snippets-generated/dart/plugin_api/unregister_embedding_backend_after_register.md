---
id: fixture_dart_unregister_embedding_backend_after_register
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

unregister_embedding_backend

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.unregisterEmbeddingBackend('test-embedding-backend');
}

```
