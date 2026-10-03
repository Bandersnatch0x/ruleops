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

    public record RegistryPin(String resource, String id, String version, String expectedDigest, String provenance, String deliveryDescriptor) {
        @Override public String toString() { return id + " @ " + version + " · " + expectedDigest + " · " + provenance; }
    }
    public record RegistryPage(List<Map<String, Object>> items, String nextCursor, boolean hasMore) {}
    private static String registryText(Object value, String name) {
        if (!(value instanceof String text) || text.isBlank() || text.chars().anyMatch(c -> c < 32 || c == 127))
            throw new IllegalArgumentException("INVALID_REGISTRY_FIELD:" + name);
        return text;
    }
    private static String registryResource(String resource) {
        if (!List.of("rules", "standards").contains(resource)) throw new IllegalArgumentException("REGISTRY_TYPE_INVALID");
        return resource;
    }
    private static String registryDigest(Object digest) {
        String value = registryText(digest, "digest");
        if (!value.matches("sha256:[a-f0-9]{64}")) throw new IllegalArgumentException("INVALID_EXPECTED_DIGEST");
        return value;
    }
    public static List<RegistryPin> projectRegistryPins(Map<String, Object> document) {
        if (!(document.get("pins") instanceof List<?> pins)) throw new IllegalArgumentException("INVALID_RELEASE_PINS");
        List<RegistryPin> result = new ArrayList<>();
        for (Object raw : pins) {
            if (!(raw instanceof Map<?, ?> pin)) throw new IllegalArgumentException("INVALID_RELEASE_PIN");
            result.add(new RegistryPin(registryResource(registryText(pin.get("resource"), "resource")),
                registryText(pin.get("id"), "id"), registryText(pin.get("version"), "version"), registryDigest(pin.get("expectedDigest")),
                registryText(pin.get("provenance"), "provenance"), pin.containsKey("deliveryDescriptor") ? registryText(pin.get("deliveryDescriptor"), "deliveryDescriptor") : ""));
        }
        return List.copyOf(result);
    }
    private static List<String> registryConnectionArgs(String server, String profile, int timeoutMs) {
        if (nonEmpty(server) == nonEmpty(profile)) throw new IllegalArgumentException("REGISTRY_CONNECTION_REQUIRED_EXACTLY_ONE");
        if (timeoutMs <= 0) throw new IllegalArgumentException("INVALID_TIMEOUT");
        if (nonEmpty(server)) {
            java.net.URI uri;
            try { uri = java.net.URI.create(server); }
            catch (IllegalArgumentException error) { throw new IllegalArgumentException("INVALID_REGISTRY_ENDPOINT"); }
            if (uri.getHost() == null || !List.of("http", "https").contains(uri.getScheme()) || uri.getUserInfo() != null || uri.getQuery() != null || uri.getFragment() != null)
                throw new IllegalArgumentException("INVALID_REGISTRY_ENDPOINT");
        }
        return List.of(nonEmpty(profile) ? "--connection-profile" : "--server", registryText(nonEmpty(profile) ? profile : server, "connection"), "--timeout-ms", String.valueOf(timeoutMs));
    }
    /** JSON string in ONE argv: opaque NUL-delimited store cursors must never be process arguments directly. */
    public static String registryCursorJson(String cursor) {
        StringBuilder result = new StringBuilder("\"");
        for (char c : cursor.toCharArray()) {
            if (c == '\\' || c == '"') result.append('\\').append(c);
            else if (c < 32 || Character.isSurrogate(c)) result.append(String.format("\\u%04x", (int)c));
            else result.append(c);
        }
        return result.append('"').toString();
    }
    public static List<String> buildRegistryListArgs(String resource, String server, String profile, int timeoutMs, int limit, String cursor) {
        if (limit < 1 || limit > 256) throw new IllegalArgumentException("REGISTRY_LIMIT_INVALID");
        List<String> args = new ArrayList<>(List.of("registry", "list", registryResource(resource)));
        args.addAll(registryConnectionArgs(server, profile, timeoutMs));
        args.addAll(List.of("--limit", String.valueOf(limit)));
        if (cursor != null) args.addAll(List.of("--cursor-json", registryCursorJson(cursor)));
        return List.copyOf(args);
    }
    public static List<String> buildRegistryShowArgs(RegistryPin pin, String server, String profile, int timeoutMs) {
        if (pin == null) throw new IllegalArgumentException("INDEPENDENT_PIN_REQUIRED");
        registryText(pin.provenance(), "provenance");
        List<String> args = new ArrayList<>(List.of("registry", "show", registryResource(pin.resource()), registryText(pin.id(), "id"),
            "--version", registryText(pin.version(), "version"), "--expected-digest", registryDigest(pin.expectedDigest())));
        args.addAll(registryConnectionArgs(server, profile, timeoutMs));
        return List.copyOf(args);
    }
    public static List<String> buildRegistrySyncArgs(RegistryPin pin, String descriptor, String repoRoot, String server, String profile, int timeoutMs) {
        if (pin == null || !"rules".equals(pin.resource())) throw new IllegalArgumentException("SYNC_RULES_ONLY");
        List<String> args = new ArrayList<>(buildRegistryShowArgs(pin, server, profile, timeoutMs)); args.set(1, "sync");
        args.addAll(List.of("--delivery-descriptor", registryText(descriptor, "deliveryDescriptor"), "--repo-root", registryText(repoRoot, "repoRoot")));
        return List.copyOf(args);
    }
    private static Map<?, ?> registryEnvelope(Map<String, Object> receipt) {
        if (!"RECEIVED_ONLY".equals(receipt.get("status")) || !"UNKNOWN".equals(receipt.get("authority"))
            || !(receipt.get("digestVerification") instanceof String verification) || !List.of("FORMAT_ONLY", "PIN_MATCH", "UNKNOWN").contains(verification))
            throw new IllegalArgumentException("REGISTRY_RESPONSE_INVALID");
        return registryData(receipt.get("registry"));
    }
    private static Map<?, ?> registryData(Object registry) {
        if (!(registry instanceof Map<?, ?> data) || !CLIENT_API_VERSION.equals(data.get("apiVersion"))
            || !"asset-registry-v1".equals(data.get("schemaVersion")) || !"UNKNOWN".equals(data.get("authority")) || !"registry-read".equals(data.get("requestId")))
            throw new IllegalArgumentException("REGISTRY_RESPONSE_INVALID");
        return data;
    }
    private static Map<String, Object> registryItem(Object raw, String resource) {
        if (!(raw instanceof Map<?, ?> item) || !registryResource(resource).equals(item.get("type"))) throw new IllegalArgumentException("REGISTRY_RESPONSE_INVALID");
        return Map.of("type", resource, "id", registryText(item.get("id"), "id"), "version", registryText(item.get("version"), "version"),
            "state", registryText(item.get("state"), "state"), "digest", registryDigest(item.get("digest")));
    }
    public static RegistryPage projectRegistryPage(Map<String, Object> receipt, String resource, String cursor) {
        var data = registryEnvelope(receipt);
        if ("PIN_MATCH".equals(receipt.get("digestVerification")) || !(data.get("items") instanceof List<?> items)
            || !(data.get("nextCursor") instanceof String next) || !java.util.Objects.equals(data.get("cursor"), cursor)
            || !(data.get("hasMore") instanceof Boolean more) || (more && (items.isEmpty() || next.equals(cursor))))
            throw new IllegalArgumentException("REGISTRY_PAGE_INVALID");
        return new RegistryPage(items.stream().map(item -> registryItem(item, resource)).toList(), next, more);
    }
    private static void matchRegistryPin(Map<?, ?> data, RegistryPin pin) {
        var item = registryItem(data.get("resource"), pin.resource());
        if (!pin.id().equals(item.get("id")) || !pin.version().equals(item.get("version"))) throw new IllegalArgumentException("REGISTRY_IDENTITY_MISMATCH · UNKNOWN · 未安装");
        if (!pin.expectedDigest().equals(item.get("digest"))) throw new IllegalArgumentException("REGISTRY_DIGEST_MISMATCH · UNKNOWN · 未安装");
    }
    public static String renderRegistryShow(Map<String, Object> receipt, RegistryPin pin) {
        matchRegistryPin(registryEnvelope(receipt), pin);
        return receipt.get("digestVerification") + " · authority=UNKNOWN · 未安装 / 不代表可执行\n" + pin;
    }
    public static String renderRegistrySync(Map<String, Object> receipt, RegistryPin pin) {
        if (!"UNKNOWN".equals(receipt.get("authority"))) throw new IllegalArgumentException("REGISTRY_SYNC_RESPONSE_INVALID");
        if (!"CACHED".equals(receipt.get("status"))) return "UNKNOWN · 未安装 / 离线可用性未知";
        if (!"PIN_MATCH".equals(receipt.get("digestVerification")) || !"SignedRuleBundle".equals(receipt.get("artifactKind"))
            || !(receipt.get("offlineAvailable") instanceof Boolean) || !"IMMUTABLE_DIGEST_ADDRESSED".equals(receipt.get("overwritePolicy"))
            || !(receipt.get("registry") instanceof Map<?, ?> registry) || !(receipt.get("provenance") instanceof Map<?, ?> provenance))
            throw new IllegalArgumentException("REGISTRY_SYNC_RESPONSE_INVALID");
        registryText(receipt.get("destination"), "destination"); registryDigest(receipt.get("bundleDigest"));
        registryText(provenance.get("descriptor"), "descriptor"); registryText(provenance.get("trustRoot"), "trustRoot");
        matchRegistryPin(registryData(registry), pin);
        return "CACHED · PIN_MATCH · authority=UNKNOWN · 缓存非安装 / 非 Gate PASS\nartifact：" + receipt.get("artifactKind")
            + "\nbundleDigest：" + receipt.get("bundleDigest") + "\ndestination：" + receipt.get("destination")
            + "\noverwritePolicy：" + receipt.get("overwritePolicy") + "\nofflineAvailable：" + receipt.get("offlineAvailable")
            + "\nprovenance：" + provenance + "\n后续扫描仍需 ExecutionLock、签名和撤销验证。";
    }

    // ---- 统一命令面：机械地从同一份 map params 构造 rf argv ----
    // 与 clients/vscode/src/core.js 消费同一份 ide-command-map.json#commands[].params；
    // 本层只机械展开参数，不新增网络或业务流程。identity 字段（actor/owner）由 UI 显式收集，绝不代填。

    public record Param(String name, String kind, String flag, boolean required,
                        List<String> values, boolean hexDigest, boolean repeatable,
                        boolean identity, boolean routeSelector) {
        public Param {
            List<String> safe = values == null ? List.of() : List.copyOf(values);
            values = safe;
        }
    }

    private static List<String> splitCsv(String raw, String name) {
        List<String> out = new ArrayList<>();
        for (String part : raw.split(",", -1)) {
            String chunk = controlFree(part.trim(), name);
            if (!chunk.isEmpty()) out.add(chunk);
        }
        return out;
    }
    private static String controlFree(String value, String name) {
        if (value == null) throw new IllegalArgumentException("INVALID_CONTROL:" + name);
        for (int i = 0; i < value.length(); i += 1)
            if (Character.isISOControl(value.charAt(i))) throw new IllegalArgumentException("INVALID_CONTROL:" + name);
        return value;
    }
    private static String digestValue(String value, String name) {
        controlFree(value, name);
        if (!value.matches("sha256:[a-f0-9]{64}")) throw new IllegalArgumentException("INVALID_DIGEST:" + name);
        return value;
    }
    private static String hexDigestValue(String value, String name) {
        controlFree(value, name);
        if (!value.matches("[a-f0-9]{64}")) throw new IllegalArgumentException("INVALID_HEX_DIGEST:" + name);
        return value;
    }
    private static String numberValue(String value, String name) {
        controlFree(value == null ? "" : value, name);
        if (value == null || !value.matches("\\d+")) throw new IllegalArgumentException("INVALID_NUMBER:" + name);
        return value;
    }

    private static final java.util.Set<String> RF_DISPATCH_WORDS = java.util.Set.of(
        "init", "doctor", "config", "registry", "standard", "rule", "coverage", "pilot", "decision",
        "center", "status", "explain", "feedback", "exception", "submission", "scan",
        "validate", "show", "list", "sync", "generate", "test", "analyze", "report", "delivery-check",
        "baseline", "observe", "governance", "window-report", "package", "adjudicate", "evaluate",
        "check", "contract-check", "deployment-check", "draft", "request", "submit");

    /** 子命令前缀 = commandTemplate 开头的已知分派词（area [+ action]），与 rf 启动器 case 分支一致。 */
    public static List<String> commandPrefix(String commandTemplate) {
        String body = commandTemplate == null ? "" : (commandTemplate.startsWith("rf ") ? commandTemplate.substring(3) : commandTemplate);
        List<String> words = new ArrayList<>();
        for (String token : body.split("\\s+")) {
            if (!RF_DISPATCH_WORDS.contains(token)) break;
            words.add(token);
        }
        if (words.size() == 1 && "pilot".equals(words.get(0)) && body.startsWith("pilot evidence ")) {
            String third = body.split("\\s+")[2];
            if ("package".equals(third) || "adjudicate".equals(third)) { words.add("evidence"); words.add(third); }
        }
        return List.copyOf(words);
    }

    /** 按 map params 声明顺序展开位置参数与 flag（与 core.js buildMappedArgs 对齐）。 */
    public static List<String> buildMappedArgs(List<Param> params, Map<String, String> values) {
        return buildMappedArgs("", params, values);
    }

    /** 同上；commandTemplate 供前缀推导。 */
    public static List<String> buildMappedArgs(String commandTemplate, List<Param> params, Map<String, String> values) {
        if (params == null) throw new IllegalArgumentException("MISSING_COMMAND_PARAMS");
        Map<String, String> input = values == null ? Map.of() : values;
        List<String> args = new ArrayList<>(commandPrefix(commandTemplate));
        for (Param p : params) {
            String raw = input.get(p.name());
            if ("bool".equals(p.kind())) {
                if ("true".equals(raw) || "1".equals(raw)) args.add(p.flag());
                continue;
            }
            if (raw == null || raw.isEmpty()) {
                if (p.required()) throw new IllegalArgumentException("MISSING_ARG:" + p.name());
                continue;
            }
            List<String> items = "csv".equals(p.kind()) ? splitCsv(raw, p.name()) : List.of(raw);
            for (String item : items) {
                String sv;
                switch (p.kind()) {
                    case "enum":
                        sv = controlFree(item, p.name());
                        if (!p.values().contains(sv)) throw new IllegalArgumentException("BAD_ENUM:" + p.name() + "=" + sv);
                        break;
                    case "digest":
                        sv = p.hexDigest() ? hexDigestValue(item, p.name()) : digestValue(item, p.name());
                        break;
                    case "number":
                        sv = numberValue(item, p.name());
                        break;
                    default:
                        // CLI path arguments come from the host and may be absolute (e.g. --repo-root);
                        // workspace-relative gating belongs to the navigation layer.
                        sv = controlFree(item, p.name());
                }
                if (p.flag() != null) { args.add(p.flag()); args.add(sv); }
                else args.add(sv);
            }
        }
        return List.copyOf(args);
    }

    /** scan：路线互斥（--rule-id / --candidate / --delivery-descriptor）、base/head 成对（与 core.js buildScanArgs 对齐）。 */
    public static List<String> buildScanArgs(String commandTemplate, List<Param> params,
                                             Map<String, String> values) {
        if (params == null) throw new IllegalArgumentException("MISSING_COMMAND_PARAMS");
        Map<String, String> input = values == null ? Map.of() : values;
        String route = input.get("route");
        if (route == null || route.isEmpty()) {
            route = nonEmpty(input.get("ruleId")) ? "rule-id"
                : nonEmpty(input.get("candidate")) ? "candidate"
                : nonEmpty(input.get("deliveryDescriptor")) ? "delivery-descriptor" : null;
        }
        java.util.Map<String, String> members = java.util.Map.of(
            "rule-id", "ruleId", "candidate", "candidate", "delivery-descriptor", "deliveryDescriptor");
        if (route == null || !members.containsKey(route)) throw new IllegalArgumentException("SCAN_ROUTE_REQUIRED");
        String memberName = members.get(route);
        if (!nonEmpty(input.get(memberName))) throw new IllegalArgumentException("MISSING_ARG:" + memberName);
        for (String other : members.values()) {
            if (!other.equals(memberName) && nonEmpty(input.get(other))) throw new IllegalArgumentException("SCAN_ROUTE_CONFLICT");
        }
        if (nonEmpty(input.get("base")) != nonEmpty(input.get("head"))) throw new IllegalArgumentException("BASE_HEAD_MUST_BE_PAIRED");
        List<String> args = new ArrayList<>(commandPrefix(commandTemplate));
        for (Param p : params) {
            boolean isRoute = p.name().equals("route") || p.name().equals(memberName) || p.name().equals("base") || p.name().equals("head");
            String raw = input.get(p.name());
            if ("bool".equals(p.kind())) {
                if (!isRoute && ("true".equals(raw) || "1".equals(raw))) args.add(p.flag());
                continue;
            }
            if (isRoute || raw == null || raw.isEmpty()) {
                continue;
            }
            List<String> items = "csv".equals(p.kind()) ? splitCsv(raw, p.name()) : List.of(raw);
            for (String item : items) {
                String sv;
                switch (p.kind()) {
                    case "enum":
                        sv = controlFree(item, p.name());
                        if (!p.values().contains(sv)) throw new IllegalArgumentException("BAD_ENUM:" + p.name() + "=" + sv);
                        break;
                    case "digest":
                        sv = p.hexDigest() ? hexDigestValue(item, p.name()) : digestValue(item, p.name());
                        break;
                    case "number":
                        sv = numberValue(item, p.name());
                        break;
                    default:
                        sv = controlFree(item, p.name());
                }
                if (p.flag() != null) { args.add(p.flag()); args.add(sv); }
                else args.add(sv);
            }
        }
        args.add("--" + route.replace("delivery-descriptor", "delivery-descriptor"));
        args.add(input.get(memberName));
        if (nonEmpty(input.get("base"))) { args.add("--base"); args.add(input.get("base")); args.add("--head"); args.add(input.get("head")); }
        return List.copyOf(args);
    }

    // ---- 扫描结果工作台语义（镜像 runtime/scan-output 契约） ----
    // 关键：绝不把 scan exit=0 当作 PASS；以 ScanResult.status/gateDecision/authority 为准，
    // ScanFailure 的 exitCode = CLI 等级（2 输入无效 / 3 未决或拒绝 / 4 信任失败）。

    public static String classifyScanExit(int code) {
        if (code == 0) return "OK";
        if (code == 2) return "INVALID_INPUT";
        if (code == 3) return "INCONCLUSIVE_OR_REFUSED";
        if (code == 4) return "TRUST_FAILED";
        return "ERROR";
    }
    public static final Map<String, String> SCAN_STATUS_LABELS = Map.of(
        "PASS", "PASS（Gate 通过）",
        "FAIL", "FAIL（存在未豁免 finding，Gate 阻止）",
        "INCONCLUSIVE", "INCONCLUSIVE（证据不足，Gate 未决）",
        "LOCAL_UNATTESTED", "LOCAL_UNATTESTED（本地未证明，非生产 Gate 证据）");

    public static final class ScanView {
        public final boolean ok;
        public final String reason;
        public final String kind;
        public final String status;
        public final String gateDecision;
        public final String authority;
        public final String trust;
        public final String ruleId;
        public final long planned, executed, failed;
        public final int exitCode;
        public final List<Map<String, Object>> findings;
        public final List<Map<String, Object>> diagnostics;
        public final String resultDigest;
        ScanView(boolean ok, String reason, String kind, String status, String gateDecision, String authority,
                 String trust, String ruleId, long planned, long executed, long failed, int exitCode,
                 List<Map<String, Object>> findings, List<Map<String, Object>> diagnostics, String resultDigest) {
            this.ok = ok; this.reason = reason; this.kind = kind; this.status = status; this.gateDecision = gateDecision;
            this.authority = authority; this.trust = trust; this.ruleId = ruleId; this.planned = planned; this.executed = executed;
            this.failed = failed; this.exitCode = exitCode; this.findings = findings; this.diagnostics = diagnostics;
            this.resultDigest = resultDigest;
        }
    }

    @SuppressWarnings("unchecked")
    private static List<Map<String, Object>> anyList(Object raw) {
        if (!(raw instanceof List<?> list)) return List.of();
        List<Map<String, Object>> out = new ArrayList<>();
        for (Object item : list) if (item instanceof Map<?, ?> map) out.add((Map<String, Object>) map);
        return List.copyOf(out);
    }
    private static long num(Object v) {
        if (v instanceof Number n) return n.longValue();
        try { return Long.parseLong(String.valueOf(v)); } catch (NumberFormatException e) { return 0; }
    }
    private static String orEmpty(Object v) { return v == null ? "" : String.valueOf(v); }
    private static String orDefault(Object v, String dflt) { return v == null ? dflt : String.valueOf(v); }

    /** 收敛既有 ScanResult/ScanFailure 结构化对象为归一化视图（closed-world）。 */
    public static ScanView normalizeScanReport(Map<String, Object> raw) {
        if (raw == null) return new ScanView(false, "NOT_OBJECT", "", "", "", "", "", "", 0, 0, 0, 0, List.of(), List.of(), "");
        String forbidden = findForbiddenField(raw);
        if (forbidden != null) return new ScanView(false, "FORBIDDEN_FIELD:" + forbidden, "", "", "", "", "", "", 0, 0, 0, 0, List.of(), List.of(), "");
        String kind = String.valueOf(raw.get("kind"));
        if ("ScanResult".equals(kind)) {
            List<String> fields = List.of("apiVersion", "kind", "status", "gateDecision", "authority", "trust", "ruleId", "findingRecords", "diagnostics", "progress", "resultDigest");
            for (String f : fields) if (!raw.containsKey(f)) return new ScanView(false, "SCAN_RESULT_INCOMPLETE", "", "", "", "", "", "", 0, 0, 0, 0, List.of(), List.of(), "");
            String status = String.valueOf(raw.get("status"));
            if (!SCAN_STATUS_LABELS.containsKey(status)) return new ScanView(false, "SCAN_STATUS_UNKNOWN:" + status, "", "", "", "", "", "", 0, 0, 0, 0, List.of(), List.of(), "");
            return new ScanView(true, null, "ScanResult", status, String.valueOf(raw.get("gateDecision")), String.valueOf(raw.get("authority")),
                String.valueOf(raw.get("trust")), String.valueOf(raw.get("ruleId")), num(raw.get("planned")), num(raw.get("executed")), num(raw.get("failed")),
                0, anyList(raw.get("findingRecords")), anyList(raw.get("diagnostics")), String.valueOf(raw.get("resultDigest")));
        }
        if ("ScanFailure".equals(kind)) {
            List<String> fields = List.of("apiVersion", "kind", "exitCode", "diagnostics", "progress", "diagnosticDigest");
            for (String f : fields) if (!raw.containsKey(f)) return new ScanView(false, "SCAN_FAILURE_INCOMPLETE", "", "", "", "", "", "", 0, 0, 0, 0, List.of(), List.of(), "");
            return new ScanView(true, null, "ScanFailure", "", "", "", "", "", 0, 0, 0,
                (int) num(raw.get("exitCode")), List.of(), anyList(raw.get("diagnostics")), "");
        }
        return new ScanView(false, "SCAN_KIND_UNKNOWN:" + kind, "", "", "", "", "", "", 0, 0, 0, 0, List.of(), List.of(), "");
    }

    /** 只读渲染扫描结果；exit 0 绝不显示为 PASS。 */
    public static String renderScanReport(Map<String, Object> raw, int exitCode, String stderr) {
        ScanView view = normalizeScanReport(raw);
        if (!view.ok) return "scan 输出无法按既有 ScanResult/ScanFailure 契约解析：" + view.reason + "\nexit=" + exitCode;
        StringBuilder progressPart = new StringBuilder();
        if (stderr != null) {
            for (String line : stderr.split("\n", -1)) {
                String t = line.trim();
                if (!t.startsWith("progress ")) continue;
                progressPart.append('\n').append(t);
            }
        }
        String step = "next_step=";
        if ("ScanFailure".equals(view.kind)) {
            StringBuilder diags = new StringBuilder();
            for (Map<String, Object> d : view.diagnostics)
                diags.append("diagnostic code=").append(orEmpty(d.get("code"))).append(" stage=").append(orEmpty(d.get("stage")))
                    .append(" retry=").append(orEmpty(d.get("retry"))).append(" message=").append(orEmpty(d.get("message")))
                    .append(" next_step=").append(orEmpty(d.get("nextStep"))).append('\n');
            return "扫描未完成（ScanFailure）出口码=" + view.exitCode + "（" + classifyScanExit(view.exitCode) + "）\n" + (diags.length() == 0 ? "" : diags.substring(0, diags.length() - 1)) + progressPart;
        }
        String statusLine = SCAN_STATUS_LABELS.get(view.status)
            + (view.gateDecision != null && !view.gateDecision.isEmpty() && !"null".equals(view.gateDecision) ? " · gate=" + view.gateDecision : "")
            + " · authority=" + view.authority + " · trust=" + view.trust;
        StringBuilder sb = new StringBuilder("scan ").append(view.status).append(" — 不做成 PASS：请以 status/gate/authority 为准\n")
            .append(statusLine).append("\nrule_id=").append(view.ruleId)
            .append(" planned=").append(view.planned).append(" executed=").append(view.executed).append(" failed=").append(view.failed)
            .append("\nresult_digest=").append(view.resultDigest);
        StringBuilder findings = new StringBuilder();
        for (Map<String, Object> f : view.findings) {
            Object locObj = f.get("location");
            String where = "identity ";
            if (locObj instanceof Map<?, ?> loc) {
                Object p = loc.get("path");
                if (p != null) where = loc.get("line") != null ? p + ":" + loc.get("line") + ":" + orDefault(loc.get("column"), "1") : String.valueOf(p);
                else where = "identity " + orEmpty(loc.get("identityDigest"));
            }
            findings.append("finding id=").append(orEmpty(f.get("findingId"))).append(" rule=").append(orEmpty(f.get("ruleId")))
                .append(" location=").append(where).append(" digest=").append(orEmpty(f.get("findingDigest")))
                .append(" message=").append(orEmpty(f.get("message"))).append('\n');
        }
        StringBuilder diags = new StringBuilder();
        for (Map<String, Object> d : view.diagnostics)
            diags.append("diagnostic code=").append(orEmpty(d.get("code"))).append(" stage=").append(orEmpty(d.get("stage")))
                .append(" retry=").append(orEmpty(d.get("retry"))).append(" message=").append(orEmpty(d.get("message")))
                .append(" next_step=").append(orEmpty(d.get("nextStep"))).append('\n');
        sb.append("\n").append(findings.length() == 0 ? "findings：无" : "findings：\n" + findings.substring(0, findings.length() - 1));
        if (diags.length() > 0) sb.append("\ndiagnostics：\n").append(diags.substring(0, diags.length() - 1));
        sb.append("\nexit=").append(exitCode).append(progressPart);
        return sb.toString();
    }


}
