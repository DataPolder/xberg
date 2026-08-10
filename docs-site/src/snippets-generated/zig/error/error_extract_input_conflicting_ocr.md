---
id: fixture_zig_error_extract_input_conflicting_ocr
language: zig
target: zig
level: typecheck
requires: []
side_effect: safe
---

extract force+disable OCR

```zig title="Zig"
const std = @import("std");
const xberg = @import("xberg");

pub fn main() !void {
    var gpa: std.heap.DebugAllocator(.{}) = .init;
    const allocator = gpa.allocator();

    const input_file_0 = try std.Io.Dir.cwd().readFileAlloc(std.testing.io, "test_documents/text/fake_text.txt", allocator, .unlimited);
    const input_file_0_json = try std.json.Stringify.valueAlloc(allocator, input_file_0, .{ .emit_strings_as_arrays = true });
    const input_json_0 = try std.mem.replaceOwned(u8, allocator, "{\"bytes\":\"__ALEF_DOC_FILE_0__\",\"config\":{\"disable_ocr\":true,\"force_ocr\":true},\"filename\":\"fake_text.txt\",\"kind\":\"bytes\",\"mime_type\":\"text/plain\"}", "\"__ALEF_DOC_FILE_0__\"", input_file_0_json);
    const _result_json = xberg.extract(input_json_0, "{\"disable_ocr\":true,\"force_ocr\":true}") catch |err| {
        std.debug.print("call failed as expected: {s}\n", .{@errorName(err)});
        return;
    };
    _ = _result_json;
}

```
