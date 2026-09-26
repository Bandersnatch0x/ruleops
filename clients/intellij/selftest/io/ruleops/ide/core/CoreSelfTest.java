package io.ruleops.ide.core;

// 离线核自测：零依赖，plain javac + java 可跑，作为门禁的 EXECUTABLE 证据。
// 与 clients/vscode/test/core.test.js 逐条对齐，任一断言失败即以非零码退出。

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public final class CoreSelfTest {
    private static int passed = 0;
    private static int failed = 0;

    public static void main(String[] args) {
        // ---- validateRelativePath 接受 ----
        assertOk("path.accept.simple", ThinClientCore.validateRelativePath("src/main/App.java").ok);
        assertOk("path.accept.nested", ThinClientCore.validateRelativePath("a/b/c/d.txt").ok);

        // ---- validateRelativePath 拒绝 ----
        assertReason("path.reject.empty", ThinClientCore.validateRelativePath(""), "EMPTY_PATH");
        assertReason("path.reject.null", ThinClientCore.validateRelativePath(null), "EMPTY_PATH");
        assertReason("path.reject.absolute", ThinClientCore.validateRelativePath("/etc/passwd"), "ABSOLUTE_PATH");
        assertReason("path.reject.windrive", ThinClientCore.validateRelativePath("C:/x"), "ABSOLUTE_PATH");
        assertReason("path.reject.backslash", ThinClientCore.validateRelativePath("a\\b"), "BACKSLASH");
        assertReason("path.reject.control", ThinClientCore.validateRelativePath("a\u0000b"), "CONTROL_CHAR");
        assertReason("path.reject.dotdot", ThinClientCore.validateRelativePath("a/../b"), "DOT_SEGMENT");
        assertReason("path.reject.dot", ThinClientCore.validateRelativePath("./a"), "DOT_SEGMENT");
        assertReason("path.reject.emptyseg", ThinClientCore.validateRelativePath("a//b"), "EMPTY_SEGMENT");

        // ---- FindingView 接受 ----
        assertOk("view.accept.minimal", ThinClientCore.validateFindingView(goodView()).ok);

        // ---- FindingView 隐私拒绝（递归） ----
        Map<String, Object> withSource = goodView();
        withSource.put("source", "public class X {}");
        assertReason("view.reject.forbidden.top", ThinClientCore.validateFindingView(withSource), "FORBIDDEN_FIELD:source");
        Map<String, Object> nested = goodView();
        Map<String, Object> loc = castMap(nested.get("location"));
        loc.put("findings", List.of("leak"));
        assertReason("view.reject.forbidden.nested", ThinClientCore.validateFindingView(nested), "FORBIDDEN_FIELD:findings");

        // ---- FindingView 闭世界/形状拒绝 ----
        Map<String, Object> unknown = goodView();
        unknown.put("extra", "x");
        assertReason("view.reject.unknown", ThinClientCore.validateFindingView(unknown), "UNKNOWN_FIELD:extra");
        Map<String, Object> badApi = goodView();
        badApi.put("apiVersion", "other/v9");
        assertReason("view.reject.apiVersion", ThinClientCore.validateFindingView(badApi), "API_VERSION_MISMATCH");
        Map<String, Object> badKind = goodView();
        badKind.put("kind", "Other");
        assertReason("view.reject.kind", ThinClientCore.validateFindingView(badKind), "KIND_MISMATCH");
        Map<String, Object> badLine = goodView();
        castMap(badLine.get("location")).put("line", 0);
        assertReason("view.reject.line", ThinClientCore.validateFindingView(badLine), "LOCATION_LINE");
        Map<String, Object> badLocPath = goodView();
        castMap(badLocPath.get("location")).put("path", "/abs");
        assertReason("view.reject.locpath", ThinClientCore.validateFindingView(badLocPath), "LOCATION_PATH:ABSOLUTE_PATH");
        assertReason("view.reject.notobject", ThinClientCore.validateFindingView("x"), "NOT_AN_OBJECT");

        // ---- 参数构造 ----
        assertEq("args.explain.json",
            ThinClientCore.buildExplainArgs("f.json", null).toString(),
            "[explain, f.json, --format, json]");
        ThinClientCore.ExplainOpts eo = new ThinClientCore.ExplainOpts();
        eo.format = "text"; eo.ruleMeta = "rm.json";
        assertEq("args.explain.text.meta",
            ThinClientCore.buildExplainArgs("f.json", eo).toString(),
            "[explain, f.json, --format, text, --rule-meta, rm.json]");
        assertEq("args.feedback.draft",
            ThinClientCore.buildFeedbackDraftArgs("f.json", "true-fp", "why", "alice", null).toString(),
            "[feedback, draft, f.json, --classification, true-fp, --reason, why, --actor, alice]");
        assertEq("args.exception.request",
            ThinClientCore.buildExceptionRequestArgs("f.json", "file", "why", "alice", null).toString(),
            "[exception, request, f.json, --scope, file, --reason, why, --actor, alice]");
        assertEq("args.submit.feedback",
            ThinClientCore.buildSubmitArgs("feedback", "req.json", "https://s", 5000, null).toString(),
            "[feedback, submit, req.json, --server, https://s, --timeout-ms, 5000]");
        assertEq("args.status.requestId",
            ThinClientCore.buildSubmissionStatusArgs("https://s", "rid-1", null, 0).toString(),
            "[submission, status, --server, https://s, --request-id, rid-1]");
        assertEq("args.status.digest",
            ThinClientCore.buildSubmissionStatusArgs("https://s", null, "sha256:ab", 0).toString(),
            "[submission, status, --server, https://s, --artifact-digest, sha256:ab]");

        // ---- 必填参数缺失抛出 ----
        assertThrows("args.feedback.missing.reason", "MISSING_ARG:reason",
            () -> ThinClientCore.buildFeedbackDraftArgs("f.json", "true-fp", "  ", "alice", null));
        assertThrows("args.feedback.bad.classification", "BAD_CLASSIFICATION:false_positive",
            () -> ThinClientCore.buildFeedbackDraftArgs("f.json", "false_positive", "why", "alice", null));
        assertThrows("args.submit.missing.server", "MISSING_ARG:serverEndpoint",
            () -> ThinClientCore.buildSubmitArgs("feedback", "req.json", "", 0, null));
        assertThrows("args.submit.badkind", "BAD_SUBMIT_KIND:bogus",
            () -> ThinClientCore.buildSubmitArgs("bogus", "req.json", "https://s", 0, null));
        assertThrows("args.status.missing.selector", "MISSING_ARG:request-id|artifact-digest",
            () -> ThinClientCore.buildSubmissionStatusArgs("https://s", null, null, 0));

        // ---- 退出码分类 ----
        assertEq("exit.0", ThinClientCore.classifyExitCode(0), "OK");
        assertEq("exit.2", ThinClientCore.classifyExitCode(2), "INVALID_INPUT");
        assertEq("exit.3", ThinClientCore.classifyExitCode(3), "REFUSAL_OR_PENDING");
        assertEq("exit.1", ThinClientCore.classifyExitCode(1), "ERROR");

        // ---- parseExplainText ----
        Map<String, String> parsed = ThinClientCore.parseExplainText("ruleId=R1\nseverity=HIGH\n\ngarbage\n");
        assertEq("parse.ruleId", parsed.get("ruleId"), "R1");
        assertEq("parse.severity", parsed.get("severity"), "HIGH");
        assertOk("parse.skips.garbage", !parsed.containsKey("garbage"));

        // ---- statusLabel fail closed ----
        assertOk("status.known", ThinClientCore.statusLabel("RECEIVED_ONLY").contains("RECEIVED_ONLY"));
        assertOk("status.unknown.failclosed", ThinClientCore.statusLabel("WEIRD").contains("fail closed"));

        // ---- receipt 守卫 ----
        ThinClientCore.ReceiptView hold = ThinClientCore.renderReceiptGuards("RECEIVED_ONLY", false, false, false);
        assertOk("receipt.hold", hold.guardsHold && hold.violations.isEmpty());
        ThinClientCore.ReceiptView viol = ThinClientCore.renderReceiptGuards("RECEIVED_ONLY", true, false, true);
        assertOk("receipt.violate", !viol.guardsHold);
        assertOk("receipt.violate.lists", viol.violations.contains("exceptionApproved") && viol.violations.contains("scmPublished"));

        System.out.println("intellij-core-selftest: pass=" + passed + " fail=" + failed);
        if (failed != 0) {
            System.exit(1);
        }
    }

    private static Map<String, Object> goodView() {
        Map<String, Object> loc = new LinkedHashMap<>();
        loc.put("path", "src/main/App.java");
        loc.put("line", 12);
        loc.put("column", 3);
        Map<String, Object> view = new LinkedHashMap<>();
        view.put("apiVersion", ThinClientCore.CLIENT_API_VERSION);
        view.put("kind", ThinClientCore.FINDING_VIEW_KIND);
        view.put("findingId", "fid-1");
        view.put("ruleId", "R1");
        view.put("fingerprint", "fp-1");
        view.put("message", "msg");
        view.put("location", loc);
        return view;
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> castMap(Object o) {
        return (Map<String, Object>) o;
    }

    private static void assertOk(String name, boolean cond) {
        if (cond) { passed += 1; } else { failed += 1; System.out.println("FAIL " + name + ": expected true"); }
    }

    private static void assertReason(String name, ThinClientCore.Check check, String reason) {
        if (!check.ok && reason.equals(check.reason)) {
            passed += 1;
        } else {
            failed += 1;
            System.out.println("FAIL " + name + ": expected fail reason=" + reason
                + " got ok=" + check.ok + " reason=" + check.reason);
        }
    }

    private static void assertEq(String name, String actual, String expected) {
        if (expected.equals(actual)) { passed += 1; }
        else { failed += 1; System.out.println("FAIL " + name + ": expected=" + expected + " actual=" + actual); }
    }

    private static void assertThrows(String name, String expectedMsg, Runnable r) {
        try {
            r.run();
            failed += 1;
            System.out.println("FAIL " + name + ": expected throw " + expectedMsg + " but returned");
        } catch (RuntimeException ex) {
            if (expectedMsg.equals(ex.getMessage())) { passed += 1; }
            else { failed += 1; System.out.println("FAIL " + name + ": expected " + expectedMsg + " got " + ex.getMessage()); }
        }
    }
}
