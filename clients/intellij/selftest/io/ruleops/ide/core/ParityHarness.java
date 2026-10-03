package io.ruleops.ide.core;

// 跨端一致性 harness（零依赖）：与 clients/vscode/test/parity-harness.js 逐行对齐。
// 由 scripts/ide-client-check.sh 编译运行并 diff 两端输出（EXECUTABLE 证据）。

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public final class ParityHarness {
    public static void main(String[] args) throws Exception {
        String rfStdout = args.length > 0 ? Files.readString(Path.of(args[0])) : "";
        Map<String, String> parsed = ThinClientCore.parseExplainText(rfStdout);

        Map<String, Object> goodView = goodView();
        Map<String, Object> forbiddenView = goodView();
        forbiddenView.put("source", "x");
        Map<String, Object> unknownView = goodView();
        unknownView.put("extra", "x");

        ThinClientCore.ExplainOpts eo = null;
        ThinClientCore.ReceiptView hold =
            ThinClientCore.renderReceiptGuards("RECEIVED_ONLY", false, false, false);
        ThinClientCore.ReceiptView violate =
            ThinClientCore.renderReceiptGuards("RECEIVED_ONLY", false, false, true);

        String[] lines = new String[] {
            "explain-args=" + String.join("|", ThinClientCore.buildExplainArgs("f.json", eo)),
            "feedback-args=" + String.join("|", ThinClientCore.buildFeedbackDraftArgs(
                "f.json", "true-fp", "why", "alice", null)),
            "exception-args=" + String.join("|", ThinClientCore.buildExceptionRequestArgs(
                "f.json", "src/Main.java", "why", "alice", null)),
            "submit-args=" + String.join("|", ThinClientCore.buildSubmitArgs(
                "feedback", "req.json", "https://s", 5000, null)),
            "status-args=" + String.join("|", ThinClientCore.buildSubmissionStatusArgs(
                "https://s", "rid-1", null, 0)),
            "exit0=" + ThinClientCore.classifyExitCode(0),
            "exit2=" + ThinClientCore.classifyExitCode(2),
            "exit3=" + ThinClientCore.classifyExitCode(3),
            "path-abs=" + pathReason("/etc/passwd"),
            "path-dotdot=" + pathReason("a/../b"),
            "path-good=" + pathReason("src/Main.java"),
            "view-good=" + viewReason(goodView),
            "view-forbidden=" + viewReason(forbiddenView),
            "view-unknown=" + viewReason(unknownView),
            "receipt-hold=" + (hold.guardsHold ? "HOLD" : String.join(",", hold.violations)),
            "receipt-violate=" + (violate.guardsHold ? "HOLD" : String.join(",", violate.violations)),
            "status-unknown=" + (ThinClientCore.statusLabel("WEIRD").contains("fail closed") ? "failclosed" : "leak"),
            "parse-finding_id=" + orEmpty(parsed.get("finding_id")),
            "parse-rule_id=" + orEmpty(parsed.get("rule_id")),
            "parse-authority_status=" + orEmpty(parsed.get("authority_status")),
            "parse-authoritative=" + orEmpty(parsed.get("authoritative")),
        };
        System.out.print(String.join("\n", lines) + "\n");
        for (String line : scopeFixtureLines(args.length > 1 ? Path.of(args[1]) : Path.of("clients/testdata/scope-options.tsv")))
            System.out.println(line);
    }

    public static List<String> scopeFixtureLines(Path fixture) throws Exception {
        List<String> output = new java.util.ArrayList<>();
        var rows = Files.readAllLines(fixture);
        for (String line : rows.subList(1, rows.size())) {
            if (line.isBlank()) continue;
            String[] columns = line.split("\t", -1);
            if (columns.length != 7) throw new AssertionError("BAD_SCOPE_FIXTURE_COLUMNS");
            Map<String, Object> view = goodView();
            view.put("location", Map.of("path", columns[1], "line", 1, "column", 1));
            var options = ThinClientCore.scopeOptions(view, Map.of("modules", items(columns[2]), "source_roots", items(columns[3])), items(columns[4]), items(columns[5]));
            String actual = String.join(";", options.stream().map(o -> o.value() + "|" + o.source() + "|" + o.broader()).toList());
            if (columns[6].equals("-")) columns[6] = ""; // Explicit empty projection avoids trailing TSV whitespace.
            if (!actual.equals(columns[6])) throw new AssertionError("SCOPE_FIXTURE:" + columns[0] + " expected=" + columns[6] + " actual=" + actual);
            output.add("scope-case-" + columns[0] + "=" + actual);
        }
        return output;
    }
    private static List<String> items(String value) { return value.isEmpty() ? List.of() : List.of(value.split(";", -1)); }

    private static String orEmpty(String s) {
        return s == null ? "" : s;
    }

    private static String pathReason(String p) {
        ThinClientCore.Check r = ThinClientCore.validateRelativePath(p);
        return r.ok ? "OK" : r.reason;
    }

    private static String viewReason(Object v) {
        ThinClientCore.Check r = ThinClientCore.validateFindingView(v);
        return r.ok ? "OK" : r.reason;
    }

    private static Map<String, Object> goodView() {
        Map<String, Object> loc = new LinkedHashMap<>();
        loc.put("path", "src/Main.java");
        loc.put("line", 3);
        loc.put("column", 1);
        Map<String, Object> view = new LinkedHashMap<>();
        view.put("apiVersion", "rulefactory.io/v1");
        view.put("kind", "FindingView");
        view.put("findingId", "fid-1");
        view.put("ruleId", "R1");
        view.put("fingerprint", "fp-1");
        view.put("message", "msg");
        view.put("location", loc);
        return view;
    }
}
