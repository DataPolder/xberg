```java title="Java"
import java.io.IOException;

public class McpServer {
    public static void main(String[] args) {
        try {
            // The Java binding is an in-process library; the MCP server is
            // provided by the `xberg` CLI, which Java can supervise directly.
            ProcessBuilder pb = new ProcessBuilder("xberg", "mcp");
            pb.inheritIO();
            Process process = pb.start();
            int exitCode = process.waitFor();
            if (exitCode != 0) {
                System.err.println("mcp exited with code " + exitCode);
            }
        } catch (IOException | InterruptedException e) {
            System.err.println("Failed to start MCP server: " + e.getMessage());
        }
    }
}
```
