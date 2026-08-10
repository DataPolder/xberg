---
id: fixture_dart_renderers_list
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

List all registered renderers

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.listRenderers();
}

```
