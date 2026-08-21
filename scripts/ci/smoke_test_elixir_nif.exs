# Prove a built musl xberg_nif shared library actually extracts documents,
# not just that :erlang.load_nif() succeeds.
#
# This deliberately bypasses Xberg.Native / RustlerPrecompiled: that module
# resolves its NIF file by downloading a matching precompiled release
# tarball from GitHub (or, with force_build, by invoking `cargo build`
# itself), neither of which tests the exact artifact this CI job just cross-
# compiled. Instead this script defines a minimal NIF stub module and loads
# the freshly built .so directly via :erlang.load_nif/2 -- the same primitive
# Rustler's generated on_load callback uses under the hood -- so the loaded
# library is provably the one produced by docker/Dockerfile.musl-rustler in
# this job, not a separately built copy.
#
# A load-only check would still be weak: :erlang.load_nif/2 resolves the
# shared object and binds every declared NIF stub by arity, which already
# catches a missing/wrong-arch .so or an ABI mismatch in exported symbol
# count -- but it does not prove a call into the library actually runs
# correctly (a panic inside Rust, or a subtly broken vendored shared-lib
# closure such as ONNX Runtime or libheif that resolves at load time but
# misbehaves at call time). This script therefore runs a real extraction
# against a known fixture and asserts the exact expected text comes back.
#
# Usage: elixir smoke_test_elixir_nif.exs <fixture-path> <expected-substring>
# Requires env var XBERG_NIF_PATH: absolute path to the built .so, WITHOUT
# the .so extension (the convention :erlang.load_nif/2 expects).
# Exit code is non-zero, with a message on stderr, on any failure: NIF load
# failure, extract_async returning {:error, _}, an empty result, or a content
# mismatch.

defmodule XbergNifSmoke do
  @moduledoc false
  @on_load :load_nif

  def load_nif do
    path =
      System.get_env("XBERG_NIF_PATH") ||
        raise "XBERG_NIF_PATH env var not set (absolute path to the .so, without extension)"

    case :erlang.load_nif(String.to_charlist(path), 0) do
      :ok -> :ok
      {:error, reason} -> raise "failed to load NIF at #{path}: #{inspect(reason)}"
    end
  end

  def extract_async(_input, _config), do: :erlang.nif_error(:nif_not_loaded)
end

defmodule Runner do
  @moduledoc false

  def fail(message) do
    IO.puts(:stderr, "SMOKE TEST FAILED: #{message}")
    System.halt(1)
  end

  def run([fixture_path, expected_substring]) do
    input_json = ~s({"kind":"uri","uri":#{Kernel.inspect(fixture_path)}})

    case XbergNifSmoke.extract_async(input_json, nil) do
      {:ok, %{results: [first | _]}} ->
        content = Map.get(first, :content)

        if is_binary(content) and String.contains?(content, expected_substring) do
          IO.puts("SMOKE TEST PASSED: found #{inspect(expected_substring)} in extracted content")
        else
          fail(
            "expected substring #{inspect(expected_substring)} not found in extracted content " <>
              "for fixture #{fixture_path}; got: #{inspect(content)}"
          )
        end

      {:ok, %{results: []}} ->
        fail("extract_async returned zero results for fixture #{fixture_path}")

      {:error, reason} ->
        fail("extract_async returned an error for fixture #{fixture_path}: #{inspect(reason)}")
    end
  end

  def run(_args) do
    fail("usage: elixir smoke_test_elixir_nif.exs <fixture-path> <expected-substring>")
  end
end

Runner.run(System.argv())
