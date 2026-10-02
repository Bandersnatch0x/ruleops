package io.ruleops.ide;

// rf 调用薄封装：仅以 ProcessBuilder 启动用户配置的 rf 启动器并回传退出码/输出。
// 不做任何权威决策；参数一律经 ThinClientCore 构造。

import com.intellij.openapi.components.Service;

import java.nio.charset.StandardCharsets;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;

@Service(Service.Level.PROJECT)
public final class RfInvoker {
    private static final int MAX_OUTPUT_BYTES = 1_048_576;

    /** rf 执行结果：退出码 + stdout/stderr 文本。 */
    public static final class Result {
        public final int exitCode;
        public final String stdout;
        public final String stderr;
        public Result(int exitCode, String stdout, String stderr) {
            this.exitCode = exitCode;
            this.stdout = stdout;
            this.stderr = stderr;
        }
    }

    public Result run(String rfPath, List<String> args, String workingDir, long timeoutMs)
            throws Exception {
        List<String> cmd = new ArrayList<>();
        cmd.add(rfPath == null || rfPath.isEmpty() ? "rf" : rfPath);
        cmd.addAll(args);
        ProcessBuilder pb = new ProcessBuilder(cmd);
        if (workingDir != null && !workingDir.isEmpty()) {
            pb.directory(new java.io.File(workingDir));
        }
        Process p = pb.start();
        ExecutorService readers = Executors.newFixedThreadPool(2);
        Future<String> stdout = readers.submit(() -> read(p.getInputStream()));
        Future<String> stderr = readers.submit(() -> read(p.getErrorStream()));
        long effectiveTimeout = timeoutMs <= 0 ? 5000 : timeoutMs;
        try {
            boolean done = p.waitFor(effectiveTimeout, TimeUnit.MILLISECONDS);
            if (!done) {
                destroyTree(p);
                return new Result(3, get(stdout), "TIMEOUT");
            }
            return new Result(p.exitValue(), stdout.get(1, TimeUnit.SECONDS), stderr.get(1, TimeUnit.SECONDS));
        } catch (Exception error) {
            destroyTree(p);
            return new Result(3, get(stdout), "OUTPUT_LIMIT");
        } finally {
            stdout.cancel(true);
            stderr.cancel(true);
            readers.shutdownNow();
            readers.awaitTermination(1, TimeUnit.SECONDS);
        }
    }

    private static String read(java.io.InputStream in) throws IOException {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        byte[] buffer = new byte[8192];
        int count;
        while ((count = in.read(buffer)) != -1) {
            if (output.size() + count > MAX_OUTPUT_BYTES) {
                throw new IOException("rf output exceeds the configured limit");
            }
            output.write(buffer, 0, count);
        }
        return output.toString(StandardCharsets.UTF_8);
    }

    private static String get(Future<String> output) {
        try {
            return output.get(1, TimeUnit.SECONDS);
        } catch (Exception ignored) {
            return "";
        }
    }

    private static void destroyTree(Process process) {
        process.toHandle().descendants().forEach(handle -> {
            try {
                handle.destroyForcibly();
            } catch (SecurityException ignored) {
                // The direct process is still terminated below.
            }
        });
        process.destroyForcibly();
    }
}
