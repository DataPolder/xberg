---
id: fixture_zig_output_format_bytes_markdown
language: zig
target: zig
level: typecheck
requires: []
side_effect: safe
---

Tests markdown output format via bytes extraction API

```zig title="Zig"
const std = @import("std");
const xberg = @import("xberg");

pub fn main() !void {
    var gpa: std.heap.DebugAllocator(.{}) = .init;
    const allocator = gpa.allocator();

    const input_file_0 = try std.Io.Dir.cwd().readFileAlloc(std.testing.io, "test_documents/pdf/fake_memo.pdf", allocator, .unlimited);
    const input_file_0_json = try std.json.Stringify.valueAlloc(allocator, input_file_0, .{ .emit_strings_as_arrays = true });
    const input_json_0 = try std.mem.replaceOwned(u8, allocator, "{\"bytes\":\"__ALEF_DOC_FILE_0__\",\"config\":{\"output_format\":\"markdown\"},\"filename\":\"fake_memo.pdf\",\"kind\":\"bytes\",\"mime_type\":\"application/pdf\"}", "\"__ALEF_DOC_FILE_0__\"", input_file_0_json);
    const _result_json = try xberg.extract(input_json_0, "{\"output_format\":\"markdown\"}");
}

```
