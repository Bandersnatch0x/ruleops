package io.ruleops.ide.core;

// ERF Thin Client 纯逻辑核（零依赖，与 IntelliJ SDK 解耦，可离线单测）。
// 权威边界镜像自 rf/erf-thin-client 与 clients/vscode/src/core.js：本核只构造 rf
// 调用、解析其输出、在导航前做路径安全门禁、并以只读姿态渲染 receipt；
// 绝不复制 Runtime/Factory/Server/Gate 权威。

import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public final class ThinClientCore {
    private ThinClientCore() {}

    public static final List<String> FORBIDDEN_FIELDS = List.of(
        "source", "sourceCode", "sourceContent", "sourceSnippet", "fullFinding", "findings");

    public static final String CLIENT_API_VERSION = "rulefactory.io/v1";
    public static final String FINDING_VIEW_KIND = "FindingView";

    /** 校验结果：ok 为 true 时 reason 为 null。 */
    public static final class Check {
        public final boolean ok;
        public final String reason;
        private Check(boolean ok, String reason) { this.ok = ok; this.reason = reason; }
        public static Check ok() { return new Check(true, null); }
        public static Check fail(String reason) { return new Check(false, reason); }
    }

    // 镜像 erf-thin-client 的 validate_relative_path：导航目标必须是工作区内相对路径。
    public static Check validateRelativePath(String path) {
        if (path == null || path.isEmpty()) {
            return Check.fail("EMPTY_PATH");
        }
        if (path.startsWith("/") || path.matches("^[A-Za-z]:.*")) {
            return Check.fail("ABSOLUTE_PATH");
        }
        if (path.indexOf('\\') >= 0) {
            return Check.fail("BACKSLASH");
        }
        if (path.indexOf(':') >= 0) {
            return Check.fail("COLON");
        }
        // 控制字符（含 NUL、制表、换行等）一律拒绝。
        for (int i = 0; i < path.length(); i += 1) {
            if (path.charAt(i) < 0x20) {
                return Check.fail("CONTROL_CHAR");
            }
        }
        for (String seg : path.split("/", -1)) {
            if (seg.equals(".") || seg.equals("..")) {
                return Check.fail("DOT_SEGMENT");
            }
            if (seg.isEmpty()) {
                return Check.fail("EMPTY_SEGMENT");
            }
        }
        return Check.ok();
    }

    // 隐私门禁：拒绝任何携带源码/完整 Finding 的对象（递归）。返回命中的字段名或 null。
    @SuppressWarnings("unchecked")
    public static String findForbiddenField(Object value) {
        if (value instanceof List<?> list) {
            for (Object item : list) {
                String hit = findForbiddenField(item);
                if (hit != null) return hit;
            }
            return null;
        }
        if (value instanceof Map<?, ?> map) {
            for (Map.Entry<?, ?> e : map.entrySet()) {
                String key = String.valueOf(e.getKey());
                if (FORBIDDEN_FIELDS.contains(key)) return key;
                String hit = findForbiddenField(e.getValue());
                if (hit != null) return hit;
            }
        }
        return null;
    }

    // 校验 FindingView 形状（闭世界：只允许既定字段），并做隐私与路径门禁。
    @SuppressWarnings("unchecked")
    public static Check validateFindingView(Object viewObj) {
        if (!(viewObj instanceof Map<?, ?> view)) {
            return Check.fail("NOT_AN_OBJECT");
        }
        String forbidden = findForbiddenField(view);
        if (forbidden != null) {
            return Check.fail("FORBIDDEN_FIELD:" + forbidden);
        }
        if (!CLIENT_API_VERSION.equals(view.get("apiVersion"))) {
            return Check.fail("API_VERSION_MISMATCH");
        }
        if (!FINDING_VIEW_KIND.equals(view.get("kind"))) {
            return Check.fail("KIND_MISMATCH");
        }
        List<String> allowed = List.of(
            "apiVersion", "kind", "findingId", "ruleId", "fingerprint", "message", "location");
        for (Object key : view.keySet()) {
            if (!allowed.contains(String.valueOf(key))) {
                return Check.fail("UNKNOWN_FIELD:" + key);
            }
        }
        Object locObj = view.get("location");
        if (!(locObj instanceof Map<?, ?> loc)) {
            return Check.fail("LOCATION_MISSING");
        }
        Object pathVal = loc.get("path");
        Check pathCheck = validateRelativePath(pathVal instanceof String s ? s : null);
        if (!pathCheck.ok) {
            return Check.fail("LOCATION_PATH:" + pathCheck.reason);
        }
        if (!isPositiveInt(loc.get("line"))) {
            return Check.fail("LOCATION_LINE");
        }
        if (!isPositiveInt(loc.get("column"))) {
            return Check.fail("LOCATION_COLUMN");
        }
        return Check.ok();
    }

    private static boolean isPositiveInt(Object v) {
        if (v instanceof Integer i) return i >= 1;
        if (v instanceof Long l) return l >= 1;
        return false;
    }

    // ---- rf 调用参数构造（子命令均为空格分隔，见 rf 启动器） ----

    /** explain 选项。null 字段表示不传该 flag；format 仅 "text"/"json"。 */
    public static final class ExplainOpts {
        public String format;
        public String ruleMeta;
        public String result;
        public String assessment;
        public String output;
    }

    public static List<String> buildExplainArgs(String findingPath, ExplainOpts opts) {
        ExplainOpts o = opts == null ? new ExplainOpts() : opts;
        List<String> args = new ArrayList<>(Arrays.asList(
            "explain", findingPath, "--format", "text".equals(o.format) ? "text" : "json"));
        if (nonEmpty(o.ruleMeta)) { args.add("--rule-meta"); args.add(o.ruleMeta); }
        if (nonEmpty(o.result)) { args.add("--result"); args.add(o.result); }
        if (nonEmpty(o.assessment)) { args.add("--assessment"); args.add(o.assessment); }
        if (nonEmpty(o.output)) { args.add("--output"); args.add(o.output); }
        return args;
    }

    private static boolean nonEmpty(String s) { return s != null && !s.isEmpty(); }

    private static void requireNonEmpty(String name, String value) {
        if (value == null || value.trim().isEmpty()) {
            throw new IllegalArgumentException("MISSING_ARG:" + name);
        }
    }

    public static List<String> buildFeedbackDraftArgs(
            String findingPath, String classification, String reason, String actor, String output) {
        requireNonEmpty("classification", classification);
        if (!List.of("true-fp", "not-fp", "scope-issue", "documentation").contains(classification)) {
            throw new IllegalArgumentException("BAD_CLASSIFICATION:" + classification);
        }
        requireNonEmpty("reason", reason);
        requireNonEmpty("actor", actor);
        List<String> args = new ArrayList<>(Arrays.asList(
            "feedback", "draft", findingPath,
            "--classification", classification, "--reason", reason, "--actor", actor));
        if (nonEmpty(output)) { args.add("--output"); args.add(output); }
        return args;
    }

    public static List<String> buildExceptionRequestArgs(
            String findingPath, String scope, String reason, String actor, String output) {
        requireNonEmpty("scope", scope);
        requireNonEmpty("reason", reason);
        requireNonEmpty("actor", actor);
        List<String> args = new ArrayList<>(Arrays.asList(
            "exception", "request", findingPath,
            "--scope", scope, "--reason", reason, "--actor", actor));
        if (nonEmpty(output)) { args.add("--output"); args.add(output); }
        return args;
    }

    // 提交动作要求显式 endpoint；为空则不构造（禁用自动发现）。
    public static List<String> buildSubmitArgs(
            String kind, String requestPath, String server, int timeoutMs, String output) {
        requireNonEmpty("serverEndpoint", server);
        if (!"feedback".equals(kind) && !"exception".equals(kind)) {
            throw new IllegalArgumentException("BAD_SUBMIT_KIND:" + kind);
        }
        List<String> args = new ArrayList<>(Arrays.asList(
            kind, "submit", requestPath, "--server", server));
        if (timeoutMs > 0) { args.add("--timeout-ms"); args.add(String.valueOf(timeoutMs)); }
        if (nonEmpty(output)) { args.add("--output"); args.add(output); }
        return args;
    }

    public static List<String> buildSubmissionStatusArgs(
            String server, String requestId, String artifactDigest, int timeoutMs) {
        requireNonEmpty("serverEndpoint", server);
        List<String> args = new ArrayList<>(Arrays.asList(
            "submission", "status", "--server", server));
        if (nonEmpty(requestId)) {
            args.add("--request-id"); args.add(requestId);
        } else if (nonEmpty(artifactDigest)) {
            args.add("--artifact-digest"); args.add(artifactDigest);
        } else {
            throw new IllegalArgumentException("MISSING_ARG:request-id|artifact-digest");
        }
        if (timeoutMs > 0) { args.add("--timeout-ms"); args.add(String.valueOf(timeoutMs)); }
        return args;
    }

    // 解析 `rf explain --format text` 的 key=value 输出。
    public static Map<String, String> parseExplainText(String stdout) {
        Map<String, String> out = new LinkedHashMap<>();
        String text = stdout == null ? "" : stdout;
        for (String raw : text.split("\n", -1)) {
            String line = raw.trim();
            if (line.isEmpty()) continue;
            int eq = line.indexOf('=');
            if (eq <= 0) continue;
            out.put(line.substring(0, eq), line.substring(eq + 1));
        }
        return out;
    }

    // rf/erf-thin-client 退出码语义：0 可解释；2 输入非法；3 拒绝/PENDING/UNKNOWN。
    public static String classifyExitCode(int code) {
        if (code == 0) return "OK";
        if (code == 2) return "INVALID_INPUT";
        if (code == 3) return "REFUSAL_OR_PENDING";
        return "ERROR";
    }

    // 已知申请/提交状态到可读标签的映射；未知状态 fail closed（不渲染为成功）。
    public static final Map<String, String> STATUS_LABELS = Map.of(
        "PROPOSED", "已起草（PROPOSED，尚未提交）",
        "REQUESTED", "已生成申请（REQUESTED，尚未提交）",
        "RECEIVED_ONLY", "仅已接收（RECEIVED_ONLY，非批准/非 Gate PASS/非 SCM 写入）",
        "REPLAYED", "重复请求返回既有资源（REPLAYED）",
        "PENDING", "待定（PENDING）",
        "UNKNOWN", "结果未知（UNKNOWN，不作成功处理）");

    public static String statusLabel(String status) {
        String label = STATUS_LABELS.get(status);
        return label != null ? label : ("未识别状态（" + status + "，fail closed）");
    }

    /** 只读渲染 SubmissionReceipt 的结果。 */
    public static final class ReceiptView {
        public final String status;
        public final boolean guardsHold;
        public final List<String> violations;
        public final String note;
        ReceiptView(String status, boolean guardsHold, List<String> violations, String note) {
            this.status = status;
            this.guardsHold = guardsHold;
            this.violations = violations;
            this.note = note;
        }
    }

    // 只读渲染 SubmissionReceipt：三条虚假权威守卫必须恒为 false，否则视为契约违背。
    public static ReceiptView renderReceiptGuards(
            String status, boolean exceptionApproved, boolean gateMutated, boolean scmPublished) {
        List<String> violations = new ArrayList<>();
        if (exceptionApproved) violations.add("exceptionApproved");
        if (gateMutated) violations.add("gateMutated");
        if (scmPublished) violations.add("scmPublished");
        return new ReceiptView(
            statusLabel(status),
            violations.isEmpty(),
            violations,
            "提交回执仅代表“已接收/已记录”，绝不表示 Feedback 已采纳、Exception 已批准或 Gate PASS。");
    }
}
