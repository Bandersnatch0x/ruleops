package io.ruleops.javahost;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Entry point of the Java Backend Host process. Configuration:
 * ERF_REPO_ROOT — repository snapshot root the unit files resolve against;
 * ERF_RULESET — compiled ruleset artifact the runtime verified;
 * ERF_SUPPORTED_MAJOR — protocol major accepted (default 0, tests only).
 */
public final class JavaHostMain {

    public static void main(String[] args) throws Exception {
        Path repoRoot = Path.of(System.getenv().getOrDefault("ERF_REPO_ROOT", "."));
        String rulesetEnv = System.getenv("ERF_RULESET");
        int supportedMajor = Integer.parseInt(
                System.getenv().getOrDefault("ERF_SUPPORTED_MAJOR", "0"));
        long maxFrameBytes = Long.parseLong(
                System.getenv().getOrDefault("ERF_MAX_FRAME_BYTES", "16777216"));
        long delayMs = Long.parseLong(
                System.getenv().getOrDefault("ERF_DELAY_MS", "0"));

        Framing framing = new Framing(System.in, System.out, maxFrameBytes);
        PmdBackendAdapter adapter;
        if (rulesetEnv != null && Files.isRegularFile(Path.of(rulesetEnv))) {
            adapter = PmdBackendAdapter.load(repoRoot, Path.of(rulesetEnv));
        } else {
            adapter = PmdBackendAdapter.empty();
        }
        JavaBackendHost host = new JavaBackendHost(framing, supportedMajor, adapter, delayMs);

        Thread reader = new Thread(host::readLoop, "erf-host-reader");
        reader.setDaemon(true);
        reader.start();
        host.dispatchLoop();
    }

    private JavaHostMain() {}
}
