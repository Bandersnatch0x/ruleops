package io.ruleops.ide;

import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.components.Service;
import com.intellij.openapi.progress.ProcessCanceledException;
import com.intellij.openapi.progress.ProgressIndicator;
import com.intellij.openapi.progress.Task;
import com.intellij.openapi.project.Project;
import org.jetbrains.annotations.NotNull;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;
import java.util.function.Consumer;

@Service(Service.Level.PROJECT)
public final class RfInvoker {
    private static final int MAX_OUTPUT_BYTES = 1_048_576;
    private static final long LOCAL_TIMEOUT_MS = 600_000L;
    public static final class UnavailableException extends IOException {
        UnavailableException(IOException cause) { super(cause.getMessage(), cause); }
    }
    public record Result(int exitCode, String stdout, String stderr) {}
    @FunctionalInterface public interface Work<T> { T run(ProgressIndicator indicator) throws Exception; }
    public static final class Job {
        private volatile ProgressIndicator indicator;
        private volatile boolean cancelled;
        public void cancel() { cancelled = true; if (indicator != null) indicator.cancel(); }
    }

    // Task callbacks are EDT callbacks; work, stream reads and process waits never run there.
    public <T> Job submit(Project project, Work<T> work, Consumer<T> success,
                          Consumer<Exception> failure, Runnable cancelled) {
        Job job = new Job();
        new Task.Backgroundable(project, "ERF", true) {
            private T result;
            private Exception error;
            @Override public void run(@NotNull ProgressIndicator indicator) {
                job.indicator = indicator;
                indicator.setIndeterminate(true);
                if (job.cancelled) indicator.cancel();
                indicator.checkCanceled();
                try { result = work.run(indicator); indicator.checkCanceled(); }
                catch (ProcessCanceledException ex) { throw ex; }
                catch (Exception ex) { error = ex; }
            }
            @Override public void onSuccess() {
                ApplicationManager.getApplication().assertIsDispatchThread();
                if (project.isDisposed()) return;
                if (job.cancelled) cancelled.run();
                else if (error != null) failure.accept(error);
                else success.accept(result);
            }
            @Override public void onCancel() {
                ApplicationManager.getApplication().assertIsDispatchThread();
                if (!project.isDisposed()) cancelled.run();
            }
        }.queue();
        return job;
    }

    public Result run(String rfPath, List<String> args, String workingDir, long timeoutMs,
                      ProgressIndicator indicator) throws Exception {
        return run(rfPath, args, workingDir, timeoutMs, indicator, null);
    }

    public Result run(String rfPath, List<String> args, String workingDir, long timeoutMs,
                      ProgressIndicator indicator, Consumer<String> onStderrLine) throws Exception {
        if (ApplicationManager.getApplication().isDispatchThread()) throw new IllegalStateException("RF_ON_EDT");
        long effectiveTimeout = processTimeoutMs(args, timeoutMs);
        indicator.checkCanceled();
        List<String> cmd = new ArrayList<>(); cmd.add(rfPath); cmd.addAll(args);
        ProcessBuilder builder = new ProcessBuilder(cmd);
        builder.directory(new File(workingDir));
        Process process;
        try { process = builder.start(); }
        catch (IOException error) { throw new UnavailableException(error); }
        ExecutorService readers = Executors.newFixedThreadPool(2);
        Future<String> stdout = readers.submit(() -> read(process.getInputStream(), null));
        Future<String> stderr = readers.submit(() -> read(process.getErrorStream(), onStderrLine));
        long started = System.nanoTime();
        try {
            while (!process.waitFor(50, TimeUnit.MILLISECONDS)) {
                indicator.checkCanceled();
                checkReader(stdout); checkReader(stderr);
                if (TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - started) >= effectiveTimeout) throw new TimeoutException("TIMEOUT");
            }
            indicator.checkCanceled();
            return new Result(process.exitValue(), stdout.get(1, TimeUnit.SECONDS), stderr.get(1, TimeUnit.SECONDS));
        } finally {
            destroyTree(process);
            process.getInputStream().close(); process.getErrorStream().close();
            stdout.cancel(true); stderr.cancel(true); readers.shutdownNow();
        }
    }
    private static void checkReader(Future<String> reader) throws Exception {
        if (reader.isDone()) reader.get();
    }
    // HTTP request budgets must not truncate local analysis or validation processes.
    static long processTimeoutMs(List<String> args, long requestTimeoutMs) {
        if (requestTimeoutMs <= 0) throw new IllegalArgumentException("INVALID_REQUEST_TIMEOUT");
        if (args.isEmpty()) return requestTimeoutMs;
        String command = args.get(0);
        boolean scan = command.equals("scan");
        boolean local = scan || command.equals("doctor")
            || (args.size() > 1 && List.of("config", "standard", "rule").contains(command)
                && (args.get(1).equals("validate") || command.equals("config") && args.get(1).equals("show")));
        if (!local) return requestTimeoutMs;
        int deadline = scan ? args.indexOf("--deadline-ms") : -1;
        if (deadline < 0) return LOCAL_TIMEOUT_MS;
        if (deadline + 1 >= args.size()) throw new IllegalArgumentException("INVALID_DEADLINE_MS");
        try {
            long value = Long.parseLong(args.get(deadline + 1));
            if (value <= 0) throw new IllegalArgumentException("INVALID_DEADLINE_MS");
            return value;
        } catch (NumberFormatException error) { throw new IllegalArgumentException("INVALID_DEADLINE_MS", error); }
    }
    private static String read(InputStream input, Consumer<String> onLine) throws IOException {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        ByteArrayOutputStream line = new ByteArrayOutputStream();
        byte[] buffer = new byte[8192]; int count;
        while ((count = input.read(buffer)) != -1) {
            if (output.size() + count > MAX_OUTPUT_BYTES) throw new IOException("OUTPUT_LIMIT");
            output.write(buffer, 0, count);
            if (onLine != null) for (int i = 0; i < count; i++) {
                if (buffer[i] == '\n') {
                    String text = line.toString(StandardCharsets.UTF_8);
                    onLine.accept(text.endsWith("\r") ? text.substring(0, text.length() - 1) : text);
                    line.reset();
                } else line.write(buffer[i]);
            }
        }
        if (onLine != null && line.size() > 0) onLine.accept(line.toString(StandardCharsets.UTF_8));
        return output.toString(StandardCharsets.UTF_8);
    }
    private static void destroyTree(Process process) {
        process.toHandle().descendants().forEach(ProcessHandle::destroyForcibly);
        process.destroyForcibly();
    }
}
