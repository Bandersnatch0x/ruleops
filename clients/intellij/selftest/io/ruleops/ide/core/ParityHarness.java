package io.ruleops.ide.core;

// 跨端一致性 harness（零依赖）：与 clients/vscode/test/parity-harness.js 逐行对齐。
// 由 scripts/ide-client-check.sh 编译运行并 diff 两端输出（EXECUTABLE 证据）。
// 阶段 4 追加：统一命令面 battery（同一 Param 列表构造 argv）与扫描结果语义 battery。

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
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

        List<String> lines = new ArrayList<>(List.of(
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
            "parse-authoritative=" + orEmpty(parsed.get("authoritative"))));
        lines.addAll(mapBatteryLines());
        lines.addAll(stage5BatteryLines());
        lines.add("scan-classify-0=" + ThinClientCore.classifyScanExit(0));
        lines.add("scan-classify-2=" + ThinClientCore.classifyScanExit(2));
        lines.add("scan-classify-3=" + ThinClientCore.classifyScanExit(3));
        lines.add("scan-classify-4=" + ThinClientCore.classifyScanExit(4));
        lines.add("scan-render-fail=" + oneLine(ThinClientCore.renderScanReport(scanFailFixture(), 0, "")));
        lines.add("scan-render-failure=" + oneLine(ThinClientCore.renderScanReport(scanFailureFixture(), 2, "")));

        System.out.print(String.join("\n", lines) + "\n");
        for (String line : scopeFixtureLines(args.length > 1 ? Path.of(args[1]) : Path.of("clients/testdata/scope-options.tsv")))
            System.out.println(line);
    }

    private static ThinClientCore.Param par(String name, String kind, String flag, boolean required) {
        return new ThinClientCore.Param(name, kind, flag, required, List.of(), false, false, false, false);
    }
    private static ThinClientCore.Param par(String name, String kind, String flag, boolean required, List<String> values) {
        return new ThinClientCore.Param(name, kind, flag, required, values, false, false, false, false);
    }
    private static ThinClientCore.Param parHex(String name) {
        return new ThinClientCore.Param(name, "digest", null, true, List.of(), true, false, false, false);
    }
    private static ThinClientCore.Param parRepeat(String name) {
        return new ThinClientCore.Param(name, "path", null, true, List.of(), false, true, false, false);
    }
    private static ThinClientCore.Param parIdentity(String name, String flag) {
        return new ThinClientCore.Param(name, "text", flag, true, List.of(), false, false, true, false);
    }
    private static Map<String, String> m(String... kv) {
        Map<String, String> out = new LinkedHashMap<>();
        for (int i = 0; i + 1 < kv.length; i += 2) out.put(kv[i], kv[i + 1]);
        return out;
    }
    private static String oneLine(String text) { return text.replace("\n", "\\n"); }

    /** 与 parity-harness.js#mapBatteryLines 一一对应，顺序与取值必须相同。 */
    static List<String> mapBatteryLines() {        List<String> out = new ArrayList<>();
        out.add("map-init=" + String.join("|", ThinClientCore.buildMappedArgs(
            "rf init --repo-root PATH [--non-interactive] [OPTIONS]",
            List.of(
            par("repoRoot", "path", "--repo-root", true), par("output", "path", "--output", false),
            par("language", "enum", "--language", false, List.of("java")),
            par("buildSystem", "enum", "--build-system", false, List.of("maven", "gradle")),
            par("nonInteractive", "bool", "--non-interactive", false)),
            m("repoRoot", "src", "language", "java", "buildSystem", "maven", "nonInteractive", "true"))));
        out.add("map-rule-generate=" + String.join("|", ThinClientCore.buildMappedArgs(
            "rf rule generate INTENT.yaml OUTPUT_DIR",
            List.of(
            par("intent", "file", null, true), par("outputDir", "dir", null, true)),
            m("intent", "src/i.yaml", "outputDir", "src/out"))));
        out.add("map-standard-validate=" + String.join("|", ThinClientCore.buildMappedArgs(
            "rf standard validate FILE...",
            List.of(parRepeat("files")), m("files", "src/a.yaml,src/b.yaml"))));
        out.add("map-registry-show=" + String.join("|", ThinClientCore.buildMappedArgs(
            "rf registry show rules|standards ID --version VERSION --expected-digest SHA256 (--server URL | --connection-profile FILE) [--timeout-ms N]",
            List.of(
            par("resource", "enum", null, true, List.of("rules", "standards")), par("id", "text", null, true),
            par("version", "text", "--version", true), par("expectedDigest", "digest", "--expected-digest", true)),
            m("resource", "rules", "id", "corp.java.x", "version", "1.0.0", "expectedDigest", "sha256:" + "a".repeat(64)))));
        out.add("map-decision-evaluate=" + String.join("|", ThinClientCore.buildMappedArgs(
            "rf decision evaluate INPUT_DIGEST SIGNAL NOW OUTPUT [OPTIONS]",
            List.of(
            parHex("inputDigest"), par("signal", "number", null, true), par("now", "number", null, true),
            par("output", "file", null, true), par("nearMiss", "bool", "--near-miss", false)),
            m("inputDigest", "b".repeat(64), "signal", "5", "now", "100", "output", "src/o.json", "nearMiss", "true"))));
        out.add("map-pilot-feedback=" + String.join("|", ThinClientCore.buildMappedArgs(
            "rf pilot feedback OBSERVER_OPTIONS...",
            List.of(
            par("journal", "file", "--journal", true), par("measurementId", "text", "--measurement-id", true),
            par("classification", "enum", "--classification", true, List.of("valid", "documentation", "scope", "false-positive")),
            parIdentity("actor", "--actor")),
            m("journal", "src/j.jsonl", "measurementId", "m1", "classification", "scope", "actor", "alice"))));
        return out;
    }

    /** 阶段 5 battery：与 parity-harness.js#stage5BatteryLines 一一对应，顺序与取值必须相同。 */
    static List<String> stage5BatteryLines() {
        List<ThinClientCore.Param> scanParams = List.of(
            new ThinClientCore.Param("repoRoot", "path", "--repo-root", true, List.of(), false, false, false, false),
            new ThinClientCore.Param("route", "enum", null, true, List.of("rule-id", "candidate", "delivery-descriptor"), false, false, false, true),
            new ThinClientCore.Param("ruleId", "text", "--rule-id", false, List.of(), false, false, false, false),
            new ThinClientCore.Param("candidate", "dir", "--candidate", false, List.of(), false, false, false, false),
            new ThinClientCore.Param("deliveryDescriptor", "file", "--delivery-descriptor", false, List.of(), false, false, false, false),
            new ThinClientCore.Param("base", "text", "--base", false, List.of(), false, false, false, false),
            new ThinClientCore.Param("head", "text", "--head", false, List.of(), false, false, false, false));
        String scanTemplate = "rf scan --repo-root PATH (--rule-id RULE_ID | --candidate DIR | --delivery-descriptor FILE) [OPTIONS]";
        String cursor = "p\0a\0\"quoted\"\\path 中文";
        List<String> cursorArgs = ThinClientCore.buildRegistryListArgs("rules", "", "/profiles/p.json", 5000, 10, cursor);
        String cursorJson = cursorArgs.get(cursorArgs.indexOf("--cursor-json") + 1);
        String doctorTemplate = "rf doctor [--format FORMAT] [--scan-probe CMD] [OPTIONS]";
        List<ThinClientCore.Param> doctorParams = List.of(
            new ThinClientCore.Param("format", "enum", "--format", false, List.of("text", "json"), false, false, false, false),
            new ThinClientCore.Param("scanProbe", "text", "--scan-probe", false, List.of(), false, false, false, false));
        List<ThinClientCore.Param> showParams = List.of(
            new ThinClientCore.Param("resource", "enum", null, true, List.of("rules", "standards"), false, false, false, false),
            new ThinClientCore.Param("id", "text", null, true, List.of(), false, false, false, false),
            new ThinClientCore.Param("version", "text", "--version", true, List.of(), false, false, false, false),
            new ThinClientCore.Param("expectedDigest", "digest", "--expected-digest", true, List.of(), false, false, false, false));
        String showTemplate = "rf registry show rules|standards ID --version VERSION --expected-digest SHA256 (--server URL | --connection-profile FILE) [--timeout-ms N]";
        return List.of(
            "s5-scan-rule-id=" + String.join("|", ThinClientCore.buildScanArgs(scanTemplate, scanParams,
                m("route", "rule-id", "ruleId", "corp.java.smoke", "repoRoot", "src"))),
            "s5-scan-candidate=" + String.join("|", ThinClientCore.buildScanArgs(scanTemplate, scanParams,
                m("route", "candidate", "candidate", "src/cand", "repoRoot", "src"))),
            "s5-scan-descriptor=" + String.join("|", ThinClientCore.buildScanArgs(scanTemplate, scanParams,
                m("route", "delivery-descriptor", "deliveryDescriptor", "desc file.json", "repoRoot", "src"))),
            "s5-scan-paired=" + String.join("|", ThinClientCore.buildScanArgs(scanTemplate, scanParams,
                m("route", "rule-id", "ruleId", "r", "repoRoot", "src", "base", "abc", "head", "def"))),
            "s5-cursor-json=" + quoteJson(cursorJson),
            "s5-mapped-doctor=" + String.join("|", ThinClientCore.buildMappedArgs(doctorTemplate, doctorParams, m("format", "json"))),
            "s5-mapped-registry-show=" + String.join("|", ThinClientCore.buildMappedArgs(showTemplate, showParams,
                m("resource", "standards", "id", "corp.std.x", "version", "2.0.0", "expectedDigest", "sha256:" + "c".repeat(64)))));
    }

    /** 与 JSON.stringify 一致的 JSON 字符串编码（含 NUL \u0000 转义），零依赖实现。 */
    private static String quoteJson(String raw) {
        StringBuilder sb = new StringBuilder("\"");
        for (char c : raw.toCharArray()) {
            if (c == '\\' || c == '"') sb.append('\\').append(c);
            else if (c == '\n') sb.append("\\n");
            else if (c == '\r') sb.append("\\r");
            else if (c == '\t') sb.append("\\t");
            else if (c < 32) sb.append(String.format("\\u%04x", (int) c));
            else sb.append(c);
        }
        return sb.append('"').toString();
    }

    static Map<String, Object> scanFailFixture() {
        Map<String, Object> loc = new LinkedHashMap<>();
        loc.put("kind", "SOURCE_SPAN"); loc.put("path", "src/Main.java"); loc.put("line", 3);
        loc.put("column", 1); loc.put("identityDigest", "IDG");
        Map<String, Object> f = new LinkedHashMap<>();
        f.put("findingId", "f1"); f.put("ruleId", "corp.java.smoke"); f.put("fingerprint", "fp1");
        f.put("findingDigest", "DDG"); f.put("message", "system exit"); f.put("location", loc);
        Map<String, Object> d = new LinkedHashMap<>();
        d.put("code", "FINDINGS_PRESENT"); d.put("stage", "gate"); d.put("message", "contains findings");
        d.put("retry", "Never"); d.put("nextStep", "review");
        Map<String, Object> r = new LinkedHashMap<>();
        r.put("apiVersion", "rulefactory.io/v1"); r.put("kind", "ScanResult"); r.put("status", "FAIL");
        r.put("gateDecision", "FAIL"); r.put("authority", "LOCAL_UNATTESTED"); r.put("trust", "LOCAL_UNATTESTED");
        r.put("ruleId", "corp.java.smoke"); r.put("planned", 1L); r.put("executed", 1L); r.put("failed", 0L);
        r.put("findingRecords", List.of(f)); r.put("diagnostics", List.of(d)); r.put("progress", List.of());
        r.put("resultDigest", "RDG");
        return r;
    }
    static Map<String, Object> scanFailureFixture() {
        Map<String, Object> d = new LinkedHashMap<>();
        d.put("code", "CLI_INPUT_INVALID"); d.put("stage", "input"); d.put("message", "bad input");
        d.put("retry", "FixInput"); d.put("nextStep", "fix");
        Map<String, Object> r = new LinkedHashMap<>();
        r.put("apiVersion", "rulefactory.io/v1"); r.put("kind", "ScanFailure"); r.put("exitCode", 2);
        r.put("diagnostics", List.of(d)); r.put("progress", List.of()); r.put("diagnosticDigest", "X");
        return r;
    }

    public static List<String> scopeFixtureLines(Path fixture) throws Exception {
        List<String> output = new ArrayList<>();
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
