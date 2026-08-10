---
id: fixture_dart_list_reranker_backends
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

List all registered reranker backends

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.listRerankerBackends();
}

```
