package io.ruleops.ide;

import io.ruleops.ide.core.ThinClientCore;
import java.nio.file.*;
import java.util.*;

/** Real filesystem checks; does not claim an IntelliJ SDK or installed-IDE test. */
public final class WorkflowFilesSelfTest {
    private static int passed;
    public static void main(String[] args) throws Exception {
        Path evidence = args.length == 0 ? Path.of("clients/intellij/build/stage2-selftest") : Path.of(args[0]);
        Path root = Files.createTempDirectory(Files.createDirectories(evidence.toAbsolutePath()), "workspace-");
        Path source = Files.createDirectories(root.resolve("app/src"));
        Path file = Files.writeString(source.resolve("A.java"), "class A {}\n");
        Map<String, Object> finding = new LinkedHashMap<>(Map.of("apiVersion", "rulefactory.io/v1", "kind", "FindingView", "findingId", "f", "ruleId", "r", "fingerprint", "fp", "message", "m",
            "location", Map.of("path", "app/src/A.java", "line", 1, "column", 1)));
        Map<String, Object> config = Map.of("modules", List.of("app"), "source_roots", List.of("app/src"));
        var options = WorkflowFiles.scopes(root, finding, config).options();
        check(options.size() == 3 && options.getFirst().value().equals("app/src/A.java"), "scopes.real.file.module");
        rejects(() -> WorkflowFiles.workspacePath(root, "../secret", false), "path.escape");
        rejects(() -> WorkflowFiles.workspacePath(root, "app/src/*.java", false), "path.glob");
        rejects(() -> WorkflowFiles.workspacePath(root, "app/src/absent", false), "path.missing");
        Files.createSymbolicLink(root.resolve("linked"), source);
        rejects(() -> WorkflowFiles.workspacePath(root, "linked/A.java", false), "symlink.parent");
        Files.createSymbolicLink(source.resolve("link.java"), file);
        rejects(() -> WorkflowFiles.workspacePath(root, "app/src/link.java", false), "symlink.file");
        Path output = root.resolve("request.json");
        var stamp = WorkflowFiles.prepareOutput(output, false, List.of(file));
        rejects(() -> WorkflowFiles.verifyWritten(stamp), "output.exit0.without.file");
        Files.writeString(output, "{\"kind\":\"FeedbackDraft\"}");
        check(WorkflowFiles.verifyWritten(stamp).equals(output), "output.real.saved");
        rejects(() -> WorkflowFiles.prepareOutput(output, false, List.of(file)), "output.overwrite.confirmation");
        rejects(() -> WorkflowFiles.prepareOutput(file, true, List.of(file)), "output.overwrites.input");
        var previous = WorkflowFiles.prepareOutput(output, true, List.of(file));
        rejects(() -> WorkflowFiles.verifyWritten(previous), "output.old.file.not.success");
        Files.createSymbolicLink(root.resolve("out-link.json"), output);
        rejects(() -> WorkflowFiles.prepareOutput(root.resolve("out-link.json"), true, List.of(file)), "output.symlink");
        var argsList = ThinClientCore.buildSubmitArgs("feedback", output.toString(), "http://127.0.0.1:8080", 1000, root.resolve("receipt.json").toString());
        check(argsList.get(2).equals(WorkflowFiles.verifyWritten(stamp).toString()), "saved.to.submit.same.path");
        Files.delete(file);
        rejects(() -> WorkflowFiles.scopes(root, finding, config), "scope.invalidated.file");
        System.out.println("intellij-filesystem-selftest: pass=" + passed + " fail=0");
    }
    private interface Checked { void run() throws Exception; }
    private static void rejects(Checked test, String name) throws Exception {
        try { test.run(); } catch (java.io.IOException | IllegalArgumentException expected) { passed++; return; }
        throw new AssertionError(name + " should reject");
    }
    private static void check(boolean value, String name) { if (!value) throw new AssertionError(name); passed++; }
}
