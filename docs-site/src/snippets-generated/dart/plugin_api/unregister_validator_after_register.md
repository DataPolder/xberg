---
id: fixture_dart_unregister_validator_after_register
language: dart
target: dart
level: typecheck
requires: []
side_effect: safe
---

unregister_validator

```dart title="Dart"
import 'package:xberg/xberg.dart';
Future<void> main() async {
  final result = await XbergBridge.unregisterValidator('test-validator');
}

```
