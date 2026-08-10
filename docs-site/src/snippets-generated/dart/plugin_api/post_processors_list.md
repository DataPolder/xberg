---
id: fixture_dart_post_processors_list
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

List all registered post-processors

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.listPostProcessors();
}

```
