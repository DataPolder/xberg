using System;
using System.Diagnostics;
using System.Threading;
using System.Threading.Tasks;

// NOTE: The C# binding has no in-process MCP server SDK (no
// XbergMcpServer/RegisterTool API). The MCP server is the `xberg mcp` CLI
// subcommand, run as a subprocess and spoken to over its stdio transport.
class McpServer
{
    public static async Task Main(string[] args)
    {
        var processInfo = new ProcessStartInfo
        {
            FileName = "xberg",
            Arguments = "mcp",
            UseShellExecute = false,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };

        using var process = Process.Start(processInfo)
            ?? throw new InvalidOperationException("Failed to start xberg mcp process");

        await Task.Delay(Timeout.Infinite);
    }
}
