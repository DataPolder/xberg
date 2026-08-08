```java title="Java"
import java.io.IOException;

public class ServeServer {
    public static void main(String[] args) {
        try {
            // The Java binding is an in-process library; the HTTP server is
            // provided by the `xberg` CLI, which Java can supervise directly.
            ProcessBuilder pb = new ProcessBuilder(
                "xberg", "serve", "--host", "0.0.0.0", "--port", "3000");
            pb.inheritIO();
            Process process = pb.start();
            int exitCode = process.waitFor();
            if (exitCode != 0) {
                System.err.println("server exited with code " + exitCode);
            }
        } catch (IOException | InterruptedException e) {
            System.err.println("Failed to start server: " + e.getMessage());
        }
    }
}
```
