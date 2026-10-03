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
            if (Character.isISOControl(path.charAt(i))) {
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
        for (String field : List.of("findingId", "ruleId", "fingerprint", "message")) {
            Object value = view.get(field);
            if (!(value instanceof String text) || text.isBlank() || text.chars().anyMatch(Character::isISOControl))
                return Check.fail("INVALID_FIELD:" + field);
        }
        Object locObj = view.get("location");
        if (!(locObj instanceof Map<?, ?> loc)) {
            return Check.fail("LOCATION_MISSING");
        }
        if (loc.size() != 3 || !loc.keySet().containsAll(List.of("path", "line", "column"))) return Check.fail("LOCATION_FIELDS");
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

    public static final List<String> CLASSIFICATIONS = List.of("true-fp", "not-fp", "scope-issue", "documentation");

    public record ScopeOption(String value, String source, boolean broader) {
        @Override public String toString() { return value + " · " + source + (broader ? " · 扩范围" : " · 当前文件"); }
    }

    private static boolean literalPath(String path) {
        return path != null && path.equals(path.trim()) && validateRelativePath(path).ok
            && !path.matches(".*[\\*?\\[\\]{}!].*");
    }

    public static Check validateScope(String value) {
        if (value == null || value.isBlank()) return Check.fail("EMPTY_SCOPE");
        String stem = value.endsWith("/**") ? value.substring(0, value.length() - 3) : value;
        return literalPath(stem) ? Check.ok() : Check.fail("UNBOUNDED_OR_INVALID_SCOPE");
    }

    /** Inputs contain only paths already checked against the workspace by the filesystem adapter. */
    public static List<ScopeOption> scopeOptions(Object finding, Map<String, ?> config,
                                                List<String> files, List<String> directories) {
        if (!validateFindingView(finding).ok) return List.of();
        Map<?, ?> location = (Map<?, ?>) ((Map<?, ?>) finding).get("location");
        String file = (String) location.get("path");
        if (!literalPath(file) || !files.contains(file)) return List.of();
        Map<String, ScopeOption> options = new LinkedHashMap<>();
        options.put(file, new ScopeOption(file, "file", false));
        int slash = file.lastIndexOf('/');
        if (slash > 0) addScope(options, file.substring(0, slash), "directory", file, directories);
        for (String field : List.of("modules", "source_roots")) {
            Object values = config.get(field);
            if (values instanceof List<?> paths) for (Object path : paths)
                if (path instanceof String value) addScope(options, value, field.equals("modules") ? "module" : "source_root", file, directories);
        }
        return List.copyOf(options.values());
    }

    private static void addScope(Map<String, ScopeOption> options, String root, String source,
                                 String file, List<String> directories) {
        if (literalPath(root) && directories.contains(root) && file.startsWith(root + "/"))
            options.putIfAbsent(root + "/**", new ScopeOption(root + "/**", source, true));
    }

    public static String selectScope(List<ScopeOption> options, String value, boolean confirmed) {
        ScopeOption selected = options.stream().filter(o -> o.value().equals(value)).findFirst()
            .orElseThrow(() -> new IllegalArgumentException("SCOPE_NOT_AVAILABLE"));
        if (selected.broader() && !confirmed) throw new IllegalArgumentException("SCOPE_CONFIRMATION_REQUIRED");
        return selected.value();
    }

    /** Refresh never silently replaces an expired selection; only a new Finding defaults to its file. */
    public static ScopeOption restoreScopeSelection(List<ScopeOption> options, String previous, boolean newFinding) {
        return options.stream().filter(option -> newFinding ? !option.broader() : option.value().equals(previous))
            .findFirst().orElse(null);
    }

    private static void validateContext(String name, String value) {
        requireNonEmpty(name, value);
        if (value.chars().anyMatch(Character::isISOControl)
                || value.getBytes(java.nio.charset.StandardCharsets.UTF_8).length > 2000)
            throw new IllegalArgumentException("INVALID_CONTEXT:" + name + " (CLI requires no control characters and at most 2000 UTF-8 bytes)");
    }

    public record Artifact(String kind, String requestId, String artifactDigest, String status) {}

    public static Artifact savedArtifact(Map<String, ?> object) {
        rejectPrivate(object);
        String kind = text(object, "kind");
        boolean feedback = "FeedbackDraft".equals(kind);
        if (!feedback && !"ExceptionRequest".equals(kind)) throw new IllegalArgumentException("INVALID_REQUEST_KIND");
        String field = feedback ? "classification" : "scope";
        List<String> fields = List.of("apiVersion", "kind", "schemaVersion", "status", "requestId", "findingDigest",
            "findingFingerprint", "ruleId", field, "reason", "actor", "digest");
        if (object.size() != fields.size() || !object.keySet().containsAll(fields)) throw new IllegalArgumentException("INVALID_REQUEST_FIELDS");
        if (!CLIENT_API_VERSION.equals(text(object, "apiVersion"))
            || !(feedback ? "feedback-draft-v1" : "exception-request-v1").equals(text(object, "schemaVersion"))
            || !(feedback ? "PROPOSED" : "REQUESTED").equals(text(object, "status")))
            throw new IllegalArgumentException("INVALID_REQUEST_CONTRACT");
        for (String key : fields) text(object, key);
        validateContext("actor", text(object, "actor")); validateContext("reason", text(object, "reason"));
        if (feedback ? !CLASSIFICATIONS.contains(text(object, field)) : !validateScope(text(object, field)).ok)
            throw new IllegalArgumentException("INVALID_REQUEST_" + field);
        digest(object, "findingDigest");
        return new Artifact(kind, digest(object, "requestId"), digest(object, "digest"), text(object, "status"));
    }

    /** CLI remains the digest/authority verifier; the UI checks shape and binds its display to the selected object. */
    public static String checkedResponse(Map<String, ?> object, Artifact expected, boolean statusQuery) {
        rejectPrivate(object);
        if (!CLIENT_API_VERSION.equals(text(object, "apiVersion"))) throw new IllegalArgumentException("INVALID_RESPONSE_API");
        String state = text(object, "status");
        for (String guard : List.of("exceptionApproved", "gateMutated", "scmPublished")) {
            Object value = object.get(guard);
            if (statusQuery ? Boolean.TRUE.equals(value) : !Boolean.FALSE.equals(value))
                throw new IllegalArgumentException("RECEIPT_AUTHORITY_GUARD:" + guard);
        }
        if (!statusQuery) {
            if (!"SubmissionReceipt".equals(text(object, "kind")) || !"submission-receipt-v1".equals(text(object, "schemaVersion"))
                || !List.of("SUBMITTED", "REPLAYED").contains(state) || !"RECEIVED_ONLY".equals(text(object, "governanceOutcome")))
                throw new IllegalArgumentException("INVALID_RECEIPT");
        } else {
            if (!"SubmissionStatus".equals(text(object, "kind")) || !"submission-status-v1".equals(text(object, "schemaVersion"))
                || !List.of("PENDING", "UNKNOWN", "RECEIVED_ONLY", "REPLAYED").contains(state)
                || !Boolean.FALSE.equals(object.get("automaticRetry"))) throw new IllegalArgumentException("INVALID_SUBMISSION_STATUS");
            String authority = List.of("PENDING", "UNKNOWN").contains(state) ? "UNKNOWN" : "RECEIVED_ONLY";
            if (!authority.equals(text(object, "authority"))) throw new IllegalArgumentException("STATUS_AUTHORITY_GUARD");
        }
        for (String key : List.of("requestId", "artifactDigest")) {
            if (!statusQuery || object.containsKey(key)) {
                String value = digest(object, key);
                if (!value.equals(key.equals("requestId") ? expected.requestId() : expected.artifactDigest()))
                    throw new IllegalArgumentException("RESPONSE_IDENTITY_CONFLICT:" + key);
            }
        }
        if (statusQuery && !object.containsKey("requestId") && !object.containsKey("artifactDigest"))
            throw new IllegalArgumentException("RESPONSE_IDENTITY_MISSING");
        return state;
    }

    private static void rejectPrivate(Map<String, ?> object) {
        String forbidden = findForbiddenField(object);
        if (forbidden != null) throw new IllegalArgumentException("FORBIDDEN_FIELD:" + forbidden);
    }
    private static String text(Map<String, ?> object, String field) {
        Object value = object.get(field);
        if (!(value instanceof String s) || s.isBlank()) throw new IllegalArgumentException("INVALID_FIELD:" + field);
        return s;
    }
    private static String digest(Map<String, ?> object, String field) {
        String value = text(object, field);
        if (!value.matches("sha256:[0-9a-f]{64}")) throw new IllegalArgumentException("INVALID_DIGEST:" + field);
        return value;
    }

    // ---- rf 调用参数构造（子命令均为空格分隔，见 rf 启动器） ----

    /** explain 选项。null 字段表示不传该 flag；format 仅 "text"/"json"。 */
    public static final class ExplainOpts {
        public String format;
        public String ruleMeta;
        public String result;
        public String authorityTrust;
        public String assessment;
        public String output;
    }

    public static List<String> buildExplainArgs(String findingPath, ExplainOpts opts) {
        ExplainOpts o = opts == null ? new ExplainOpts() : opts;
        requireNonEmpty("findingPath", findingPath);
        if (nonEmpty(o.authorityTrust) && !nonEmpty(o.result))
            throw new IllegalArgumentException("AUTHORITY_TRUST_REQUIRES_RESULT");
        if (nonEmpty(o.format) && !List.of("text", "json").contains(o.format))
            throw new IllegalArgumentException("BAD_FORMAT:" + o.format);
        List<String> args = new ArrayList<>(Arrays.asList(
            "explain", findingPath, "--format", "text".equals(o.format) ? "text" : "json"));
        if (nonEmpty(o.ruleMeta)) { args.add("--rule-meta"); args.add(o.ruleMeta); }
        if (nonEmpty(o.result)) { args.add("--result"); args.add(o.result); }
        if (nonEmpty(o.authorityTrust)) { args.add("--authority-trust"); args.add(o.authorityTrust); }
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
        requireNonEmpty("findingPath", findingPath);
        requireNonEmpty("classification", classification);
        if (!CLASSIFICATIONS.contains(classification)) {
            throw new IllegalArgumentException("BAD_CLASSIFICATION:" + classification);
        }
        validateContext("reason", reason);
        validateContext("actor", actor);
        List<String> args = new ArrayList<>(Arrays.asList(
            "feedback", "draft", findingPath,
            "--classification", classification, "--reason", reason, "--actor", actor));
        if (nonEmpty(output)) { args.add("--output"); args.add(output); }
        return args;
    }

    public static List<String> buildExceptionRequestArgs(
            String findingPath, String scope, String reason, String actor, String output) {
        requireNonEmpty("findingPath", findingPath);
        requireNonEmpty("scope", scope);
        Check scopeCheck = validateScope(scope);
        if (!scopeCheck.ok) throw new IllegalArgumentException("BAD_SCOPE:" + scopeCheck.reason);
        validateContext("reason", reason);
        validateContext("actor", actor);
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
        if (nonEmpty(requestId) && nonEmpty(artifactDigest))
            throw new IllegalArgumentException("CONFLICTING_SELECTORS");
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
