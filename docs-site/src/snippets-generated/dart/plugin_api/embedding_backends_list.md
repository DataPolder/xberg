---
id: fixture_dart_embedding_backends_list
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

List all registered embedding backends

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.listEmbeddingBackends();
}

```
