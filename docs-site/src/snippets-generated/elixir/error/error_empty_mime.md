---
id: fixture_elixir_error_empty_mime
language: elixir
target: elixir
level: typecheck
requires: []
side_effect: safe
---

Show how an empty MIME type is rejected consistently.

```elixir title="Elixir"
input_value = %Xberg.ExtractInput{bytes: File.read!("test_documents/text/plain.txt"), config: %{}, filename: "plain.txt", kind: "bytes", mime_type: ""}
result = Xberg.extract_async(input_value, "{}")

```
