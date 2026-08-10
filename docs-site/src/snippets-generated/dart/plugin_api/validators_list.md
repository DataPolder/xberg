---
id: fixture_dart_validators_list
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

List all registered validators

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.listValidators();
}

```
