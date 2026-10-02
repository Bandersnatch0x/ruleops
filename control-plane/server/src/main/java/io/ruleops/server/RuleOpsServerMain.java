package io.ruleops.server;

import java.nio.file.Path;

/** Small executable for local development; it does not grant production authority. */
public final class RuleOpsServerMain {
    public static void main(String[] args) throws Exception {
        if (args.length > 2) {
            throw new IllegalArgumentException("usage: RuleOpsServerMain [port] [journal-path]");
        }
        int port = args.length == 0 ? 8080 : Integer.parseInt(args[0]);
        RuleOpsJournal journal = args.length == 2
                ? new FileRuleOpsJournal(Path.of(args[1]))
                : RuleOpsJournal.noop();
        RuleOpsStore store;
        try {
            store = new RuleOpsStore(journal);
        } catch (Exception error) {
            journal.close();
            throw error;
        }
        RuleOpsHttpServer server = RuleOpsHttpServer.loopback(port, store);
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            server.close();
            try {
                journal.close();
            } catch (Exception ignored) {
                // Shutdown hooks cannot report a second failure safely.
            }
        }, "ruleops-server-shutdown"));
        server.start();
        System.out.println("ruleops-server=" + server.uri());
        if (args.length == 2) {
            System.out.println("ruleops-journal=" + ((FileRuleOpsJournal) journal).path());
        }
        Thread.currentThread().join();
    }

    private RuleOpsServerMain() {}
}
