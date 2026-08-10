---
id: fixture_zig_extract_batch_bytes_size_cap
language: zig
target: zig
level: typecheck
requires: []
side_effect: safe
---

extract_batch: archive size cap triggers error

```zig title="Zig"
const std = @import("std");
const xberg = @import("xberg");

pub fn main() !void {
    var gpa: std.heap.DebugAllocator(.{}) = .init;
    const allocator = gpa.allocator();

    const inputs_file_0 = try std.Io.Dir.cwd().readFileAlloc(std.testing.io, "test_documents/text/fake_text.txt", allocator, .unlimited);
    const inputs_file_0_json = try std.json.Stringify.valueAlloc(allocator, inputs_file_0, .{ .emit_strings_as_arrays = true });
    const inputs_json_0 = try std.mem.replaceOwned(u8, allocator, "[{\"bytes\":\"__ALEF_DOC_FILE_0__\",\"kind\":\"bytes\",\"mime_type\":\"text/plain\"}]", "\"__ALEF_DOC_FILE_0__\"", inputs_file_0_json);
    const _result_json = xberg.extract_batch(inputs_json_0, "{\"security_limits\":{\"max_content_size\":1}}") catch |err| {
        std.debug.print("call failed as expected: {s}\n", .{@errorName(err)});
        return;
    };
    _ = _result_json;
}

```
