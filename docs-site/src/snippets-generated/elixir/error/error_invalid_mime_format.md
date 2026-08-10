---
id: fixture_elixir_error_invalid_mime_format
language: elixir
target: elixir
level: typecheck
requires: []
side_effect: safe
---

Error when extracting with invalid MIME type format

```elixir title="Elixir"
input_value = %Xberg.ExtractInput{bytes: File.read!("test_documents/text/plain.txt"), config: %{}, filename: "plain.txt", kind: "bytes", mime_type: "not-a-mime"}
result = Xberg.extract_async(input_value, "{}")

```
