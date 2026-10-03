package io.ruleops.ide;

import io.ruleops.ide.core.ThinClientCore;
import java.util.*;

/**
 * 阶段 5：阶段 3/4 新增面的负例与边界自测（与 clients/vscode/test/stage5-negatives.test.js 对齐）。
 * 重点：buildScanArgs / buildMappedArgs 边界、registry NUL cursor 与分页、PIN mismatch、
 * 缺独立 pin、schema 非法、UNKNOWN 查询回执。零依赖，plain javac + java 可跑。
 */
public final class Stage5SelfTest {
    private static int passed;
    private static int failed;

    private static final String DIGEST = "sha256:" + "a".repeat(64);
    private static final String OTHER = "sha256:" + "b".repeat(64);
    private static final ThinClientCore.Param ROUTE =
        new ThinClientCore.Param("route", "enum", null, true, List.of("rule-id", "candidate", "delivery-descriptor"), false, false, false, true);
    private static final ThinClientCore.Param RULE_ID = new ThinClientCore.Param("ruleId", "text", "--rule-id", false, List.of(), false, false, false, false);
    private static final ThinClientCore.Param CANDIDATE = new ThinClientCore.Param("candidate", "dir", "--candidate", false, List.of(), false, false, false, false);
    private static final ThinClientCore.Param DESCRIPTOR = new ThinClientCore.Param("deliveryDescriptor", "file", "--delivery-descriptor", false, List.of(), false, false, false, false);
    private static final ThinClientCore.Param BASE = new ThinClientCore.Param("base", "text", "--base", false, List.of(), false, false, false, false);
    private static final ThinClientCore.Param HEAD = new ThinClientCore.Param("head", "text", "--head", false, List.of(), false, false, false, false);
    private static final List<ThinClientCore.Param> SCAN_PARAMS = List.of(ROUTE, RULE_ID, CANDIDATE, DESCRIPTOR, BASE, HEAD);

    private static Map<String, Object> changed(Map<String, Object> original, String key, Object value) {
        var copy = new LinkedHashMap<String, Object>(original);
        copy.put(key, value);
        return copy;
    }

    private static Map<String, Object> envelope(Map<String, Object> data, String verification) {
        var registry = new LinkedHashMap<String, Object>(Map.of("apiVersion", "rulefactory.io/v1",
            "schemaVersion", "asset-registry-v1", "authority", "UNKNOWN", "requestId", "registry-read"));
        registry.putAll(data);
        return Map.of("status", "RECEIVED_ONLY", "authority", "UNKNOWN", "digestVerification", verification, "registry", registry);
    }

    private static Map<String, Object> item() {
        return Map.of("type", "rules", "id", "corp.java.rule", "version", "1.0.0", "state", "PUBLISHED", "digest", DIGEST);
    }

    private static Map<String, Object> page(String cursor, String next, boolean more) {
        return envelope(Map.of("cursor", cursor, "nextCursor", next, "hasMore", more, "items", List.of(item())), "FORMAT_ONLY");
    }

    private static Map<String, Object> show(Map<String, Object> itemOverride) {
        return envelope(Map.of("resource", itemOverride), "PIN_MATCH");
    }

    private static Map<String, Object> pin() {
        return Map.of("resource", "rules", "id", "corp.java.rule", "version", "1.0.0",
            "expectedDigest", DIGEST, "provenance", "release 7");
    }

    public static void main(String[] args) {
        // ---- buildScanArgs：三路线互斥与 base/head 成对 ----
        check("scan rule-id route argv", String.join("|", ThinClientCore.buildScanArgs("rf scan", SCAN_PARAMS,
            Map.of("route", "rule-id", "ruleId", "corp.java.smoke"))).contains("--rule-id|corp.java.smoke"));
        check("scan candidate route flag", ThinClientCore.buildScanArgs("rf scan", SCAN_PARAMS,
            Map.of("route", "candidate", "candidate", "src/cand")).contains("--candidate"));
        check("scan descriptor route flag", ThinClientCore.buildScanArgs("rf scan", SCAN_PARAMS,
            Map.of("route", "delivery-descriptor", "deliveryDescriptor", "desc.json")).contains("--delivery-descriptor"));
        check("scan paired base/head accepted", ThinClientCore.buildScanArgs("rf scan", SCAN_PARAMS,
            Map.of("route", "rule-id", "ruleId", "r", "base", "abc", "head", "def")).contains("--base"));
        rejects("scan.missingRoute", "SCAN_ROUTE_REQUIRED",
            () -> ThinClientCore.buildScanArgs("rf scan", SCAN_PARAMS, Map.of()));
        rejects("scan.conflict", "SCAN_ROUTE_CONFLICT",
            () -> ThinClientCore.buildScanArgs("rf scan", SCAN_PARAMS, Map.of("route", "rule-id", "ruleId", "r", "candidate", "c")));
        rejects("scan.baseWithoutHead", "BASE_HEAD_MUST_BE_PAIRED",
            () -> ThinClientCore.buildScanArgs("rf scan", SCAN_PARAMS, Map.of("route", "rule-id", "ruleId", "r", "base", "abc")));
        rejects("scan.headWithoutBase", "BASE_HEAD_MUST_BE_PAIRED",
            () -> ThinClientCore.buildScanArgs("rf scan", SCAN_PARAMS, Map.of("route", "rule-id", "ruleId", "r", "head", "def")));

        // ---- buildMappedArgs 边界：必填缺失/枚举非法/digest 形状/控制字符/number ----
        List<ThinClientCore.Param> initParams = List.of(
            new ThinClientCore.Param("repoRoot", "path", "--repo-root", true, List.of(), false, false, false, false),
            new ThinClientCore.Param("language", "enum", "--language", false, List.of("java"), false, false, false, false),
            new ThinClientCore.Param("deadline", "number", "--deadline-ms", false, List.of(), false, false, false, false),
            new ThinClientCore.Param("digest", "digest", "--digest", false, List.of(), false, false, false, false));
        rejects("mapped.missingRequired", "MISSING_ARG:repoRoot",
            () -> ThinClientCore.buildMappedArgs("rf init", initParams, Map.of("language", "java")));
        rejects("mapped.badEnum", "BAD_ENUM:language=scala",
            () -> ThinClientCore.buildMappedArgs("rf init", initParams, Map.of("repoRoot", "src", "language", "scala")));
        rejects("mapped.badNumber", "INVALID_NUMBER:deadline",
            () -> ThinClientCore.buildMappedArgs("rf init", initParams, Map.of("repoRoot", "src", "deadline", "1.5")));
        rejects("mapped.badDigest", "INVALID_DIGEST:digest",
            () -> ThinClientCore.buildMappedArgs("rf init", initParams, Map.of("repoRoot", "src", "digest", "md5:zz")));
        rejects("mapped.hexDigestShape", "INVALID_DIGEST:digest",
            () -> ThinClientCore.buildMappedArgs("rf init", initParams,
                Map.of("repoRoot", "src", "digest", "b".repeat(64)))); // 该 Param 未开 hexDigest：裸 64 位 hex 不合法，必须 sha256: 前缀。
        rejects("mapped.controlChar", "INVALID_CONTROL:repoRoot",
            () -> ThinClientCore.buildMappedArgs("rf init", initParams, Map.of("repoRoot", "src\nx")));
        check("mapped optional flags in declared order", String.join("|", ThinClientCore.buildMappedArgs("rf init", initParams,
            Map.of("repoRoot", "src", "deadline", "500", "digest", DIGEST))).equals("init|--repo-root|src|--deadline-ms|500|--digest|" + DIGEST));
        rejects("mapped.nullRequired", "MISSING_ARG:repoRoot",
            () -> ThinClientCore.buildMappedArgs("rf init", initParams, null));

        // ---- registry：NUL cursor 与分页边界 ----
        String cursor = "a\0b\"\\c\nd";
        List<String> list = ThinClientCore.buildRegistryListArgs("rules", "", "/profiles/p.json", 5000, 10, cursor);
        check("NUL JSON-escaped in one argv", list.get(list.indexOf("--cursor-json") + 1).contains("\\u0000"));
        boolean rawControl = false;
        for (String value : list) for (int i = 0; i < value.length(); i += 1) rawControl |= value.charAt(i) < 32 || value.charAt(i) == 127;
        check("argv free of raw control characters", !rawControl);
        rejects("page.echoMismatch", "PAGE_INVALID",
            () -> ThinClientCore.projectRegistryPage(page("other", "n2", true), "rules", ""));
        rejects("page.hasMoreEmptyItems", "PAGE_INVALID",
            () -> ThinClientCore.projectRegistryPage(envelope(Map.of("cursor", "", "nextCursor", "n", "hasMore", true, "items", List.of()), "FORMAT_ONLY"), "rules", ""));
        check("last page accepted", !ThinClientCore.projectRegistryPage(page("", "end", false), "rules", "").hasMore());

        // ---- registry：PIN mismatch / 缺独立 pin / schema 非法 ----
        var pins = ThinClientCore.projectRegistryPins(Map.of("pins", List.of(pin())));
        rejects("show.digestMismatch", "DIGEST_MISMATCH",
            () -> ThinClientCore.renderRegistryShow(show(changed(item(), "digest", OTHER)), pins.getFirst()));
        rejects("show.identityMismatch", "IDENTITY_MISMATCH",
            () -> ThinClientCore.renderRegistryShow(show(changed(item(), "version", "9.9.9")), pins.getFirst()));
        rejects("pin.empty", "INDEPENDENT_PIN_REQUIRED",
            () -> ThinClientCore.buildRegistryShowArgs(null, "", "/profiles/p.json", 5000));
        // standards pin 的 show 在 Java 核是合法命令（registryResource 接受 standards）；
        // 只有 sync 限定 rules（SYNC_RULES_ONLY）。与 VS Code selectRegistryPin 的差异已记入报告。
        check("pin.standardsShowAllowed", !ThinClientCore.buildRegistryShowArgs(
            new ThinClientCore.RegistryPin("standards", "i", "v", DIGEST, "p", ""), "", "/profiles/p.json", 5000).isEmpty());
        rejects("pins.schemaNotList", "INVALID_RELEASE_PINS", () -> ThinClientCore.projectRegistryPins(Map.of("pins", "x")));
        rejects("pins.schemaBadDigest", "INVALID_EXPECTED_DIGEST",
            () -> ThinClientCore.projectRegistryPins(Map.of("pins", List.of(changed(pin(), "expectedDigest", "md5:zz")))));
        rejects("pins.schemaControlProvenance", "INVALID_REGISTRY_FIELD",
            () -> ThinClientCore.projectRegistryPins(Map.of("pins", List.of(changed(pin(), "provenance", "rel\0ease")))));
        rejects("sync.standardsOnly", "SYNC_RULES_ONLY",
            () -> ThinClientCore.buildRegistrySyncArgs(new ThinClientCore.RegistryPin("standards", "i", "v", DIGEST, "p", ""), "/d", "/repo", "", "/p", 5000));

        // ---- registry：endpoint 非法（鉴权内嵌/查询/哈希/非 http(s)）----
        for (String bad : List.of("https://u:p@example.com/v1", "http://127.0.0.1/v1?token=x", "https://example.com/v1#frag", "ftp://example.com"))
            rejects("endpoint." + bad, "ENDPOINT", () -> ThinClientCore.buildRegistryListArgs("rules", bad, "", 5000, 10, null));

        // ---- submission status：UNKNOWN 查询回执 fail closed ----
        var identity = new ThinClientCore.Artifact("FeedbackDraft", DIGEST, OTHER, "PROPOSED");
        Map<String, Object> status = new LinkedHashMap<>(Map.of(
            "apiVersion", "rulefactory.io/v1", "kind", "SubmissionStatus", "schemaVersion", "submission-status-v1",
            "status", "UNKNOWN", "requestId", DIGEST, "automaticRetry", false, "authority", "UNKNOWN"));
        check("status UNKNOWN accepted as UNKNOWN", "UNKNOWN".equals(ThinClientCore.checkedResponse(status, identity, true)));
        check("UNKNOWN label never success", ThinClientCore.statusLabel("UNKNOWN").contains("不作成功处理"));
        rejects("status.authorityContradiction", "STATUS_AUTHORITY_GUARD",
            () -> ThinClientCore.checkedResponse(changed(status, "authority", "RECEIVED_ONLY"), identity, true));
        rejects("status.autoRetryTrue", "INVALID_SUBMISSION_STATUS",
            () -> ThinClientCore.checkedResponse(changed(status, "automaticRetry", true), identity, true));
        rejects("status.approvedState", "INVALID_SUBMISSION_STATUS",
            () -> ThinClientCore.checkedResponse(changed(status, "status", "APPROVED"), identity, true));
        var noIdentity = new LinkedHashMap<String, Object>(status);
        noIdentity.remove("requestId");
        rejects("status.identityMissing", "RESPONSE_IDENTITY_MISSING", () -> ThinClientCore.checkedResponse(noIdentity, identity, true));

        // ---- schema 非法：receipt schemaVersion / apiVersion / kind 错误 ----
        Map<String, Object> receipt = new LinkedHashMap<>(Map.of(
            "apiVersion", "rulefactory.io/v1", "kind", "SubmissionReceipt", "schemaVersion", "submission-receipt-v1",
            "status", "SUBMITTED", "governanceOutcome", "RECEIVED_ONLY", "requestId", DIGEST, "artifactDigest", OTHER,
            "exceptionApproved", false, "gateMutated", false, "scmPublished", false));
        rejects("receipt.badSchema", "INVALID_RECEIPT", () -> ThinClientCore.checkedResponse(changed(receipt, "schemaVersion", "v0"), identity, false));
        rejects("receipt.badApi", "INVALID_RESPONSE_API", () -> ThinClientCore.checkedResponse(changed(receipt, "apiVersion", "other/v9"), identity, false));
        rejects("receipt.badKind", "INVALID_RECEIPT", () -> ThinClientCore.checkedResponse(changed(receipt, "kind", "SubmissionStatus"), identity, false));

        // ---- 路径越界：location.path 负例（与 VS Code 侧同组输入）----
        for (String bad : List.of("..\\..\\etc\\passwd", "/etc/passwd", "C:\\win", "a\0b", "a\nb", "src/../../x", "a//b")) {
            Map<String, Object> loc = new LinkedHashMap<>(Map.of("path", bad, "line", 1, "column", 1));
            Map<String, Object> view = new LinkedHashMap<>(Map.of(
                "apiVersion", "rulefactory.io/v1", "kind", "FindingView", "findingId", "f",
                "ruleId", "r", "fingerprint", "fp", "message", "m", "location", loc));
            check("path.reject." + bad, !ThinClientCore.validateFindingView(view).ok);
        }

        // ---- scan 输出 schema 非法 fail closed，绝不渲染 PASS ----
        String garbageRender = ThinClientCore.renderScanReport(Map.of(), 0, "");
        check("scan.garbage.notPass", !garbageRender.contains("PASS（Gate 通过）"));
        for (String reason : List.of(
            ThinClientCore.normalizeScanReport(null).reason,
            ThinClientCore.normalizeScanReport(Map.of("kind", "Mystery")).reason,
            ThinClientCore.normalizeScanReport(Map.of("kind", "ScanResult")).reason,
            ThinClientCore.normalizeScanReport(scanFixture("WEIRD")).reason)) {
            check("scan.schemaReject." + reason, reason != null && !reason.isEmpty()
                && (reason.startsWith("NOT_") || reason.contains("UNKNOWN") || reason.contains("INCOMPLETE")));
        }

        System.out.println("intellij-stage5-selftest: pass=" + passed + " fail=" + failed);
        if (failed != 0) System.exit(1);
    }

    private static Map<String, Object> scanFixture(String status) {
        Map<String, Object> r = new LinkedHashMap<>();
        r.put("apiVersion", "rulefactory.io/v1");
        r.put("kind", "ScanResult");
        r.put("status", status);
        r.put("gateDecision", status);
        r.put("authority", "LOCAL_UNATTESTED");
        r.put("trust", "LOCAL_UNATTESTED");
        r.put("ruleId", "corp.java.smoke");
        r.put("planned", 1L);
        r.put("executed", 1L);
        r.put("failed", 0L);
        r.put("findingRecords", List.of());
        r.put("diagnostics", List.of());
        r.put("progress", List.of());
        r.put("resultDigest", "RDG");
        return r;
    }

    private static void check(String name, boolean condition) {
        if (condition) passed += 1;
        else { failed += 1; System.out.println("FAIL " + name); }
    }

    private static void rejects(String name, String code, Runnable work) {
        try {
            work.run();
            failed += 1;
            System.out.println("FAIL " + name + ": expected " + code + " but returned");
        } catch (RuntimeException error) {
            if (error.getMessage() != null && error.getMessage().contains(code)) passed += 1;
            else { failed += 1; System.out.println("FAIL " + name + ": expected " + code + " got " + error.getMessage()); }
        }
    }
}
