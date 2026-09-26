package io.ruleops.server;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.URI;
import java.net.URLDecoder;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/** HTTP adapter for the stable V0.2 RuleOps domain/API boundary. */
public final class RuleOpsHttpServer implements AutoCloseable {

    public static final String API_VERSION = "rulefactory.io/v1";
    private static final int MAX_BODY_BYTES = 1024 * 1024;
    private static final int MAX_COLLECTION_RESPONSE_BYTES = 4 * 1024 * 1024;
    private static final Set<String> FORBIDDEN_PAYLOAD_FIELDS = Set.of(
            "source", "sourceCode", "sourceContent", "sourceSnippet", "fullFinding", "findings");
    private static final ObjectMapper MAPPER = new ObjectMapper();
    private final HttpServer server;
    private final ExecutorService executor;
    private final ThreadPoolExecutor deliveryExecutor;
    private final RuleOpsStore store;
    private final RuleBundleDeliveryCatalog deliveryCatalog;
    private final Map<String, PendingSubmission> pendingSubmissions = new ConcurrentHashMap<>();
    private final Set<String> replayedSubmissions = ConcurrentHashMap.newKeySet();
    private boolean started;

    public static RuleOpsHttpServer loopback(int port, RuleOpsStore store) throws IOException {
        return loopback(port, store, new RuleBundleDeliveryCatalog());
    }

    public static RuleOpsHttpServer loopback(int port, RuleOpsStore store,
            RuleBundleDeliveryCatalog deliveryCatalog) throws IOException {
        return new RuleOpsHttpServer(HttpServer.create(
                new InetSocketAddress(InetAddress.getLoopbackAddress(), port), 0), store, deliveryCatalog);
    }

    private RuleOpsHttpServer(HttpServer server, RuleOpsStore store,
            RuleBundleDeliveryCatalog deliveryCatalog) {
        this.server = server;
        this.store = store == null ? new RuleOpsStore() : store;
        this.deliveryCatalog = deliveryCatalog == null ? new RuleBundleDeliveryCatalog() : deliveryCatalog;
        this.executor = new ThreadPoolExecutor(4, 32, 60, TimeUnit.SECONDS,
                new ArrayBlockingQueue<>(256), runnable -> {
                    Thread thread = new Thread(runnable, "ruleops-server-http");
                    thread.setDaemon(true);
                    return thread;
                }, new ThreadPoolExecutor.AbortPolicy());
        this.deliveryExecutor = new ThreadPoolExecutor(2, 16, 60, TimeUnit.SECONDS,
                new ArrayBlockingQueue<>(128), runnable -> {
                    Thread thread = new Thread(runnable, "ruleops-server-delivery");
                    thread.setDaemon(true);
                    return thread;
                }, new ThreadPoolExecutor.AbortPolicy());
        server.setExecutor(executor);
        server.createContext("/v1", this::handle);
    }

    public URI uri() {
        return URI.create("http://" + server.getAddress().getHostString() + ":"
                + server.getAddress().getPort() + "/v1");
    }

    public RuleOpsStore store() {
        return store;
    }

    /** Internal publishing seam; the HTTP API remains read-only for delivery. */
    public RuleBundleDeliveryCatalog deliveryCatalog() {
        return deliveryCatalog;
    }

    public synchronized void start() {
        if (!started) {
            server.start();
            started = true;
        }
    }

    @Override
    public synchronized void close() {
        if (started) {
            server.stop(0);
            started = false;
        }
        executor.shutdownNow();
        deliveryExecutor.shutdownNow();
    }

    private void handle(HttpExchange exchange) {
        String requestId = header(exchange, "X-Request-Id").orElseGet(() -> "generated-" + UUID.randomUUID());
        boolean delegated = false;
        try {
            String relative = exchange.getRequestURI().getPath().substring("/v1".length());
            List<String> parts = pathParts(relative);
            if (parts.isEmpty() || parts.get(0).isBlank()) {
                send(exchange, 404, error("NOT_FOUND", "API resource was not found", requestId));
                return;
            }
            String method = exchange.getRequestMethod();
            if (method.equals("GET") && parts.size() == 3 && parts.get(0).equals("rule-bundles")
                    && parts.get(2).equals("delivery")) {
                delegated = dispatchDelivery(exchange, parts.get(1), requestId);
                return;
            }
            if (method.equals("GET") && parts.size() == 1 && parts.get(0).equals("health")) {
                ObjectNode response = base();
                response.put("status", "ok");
                response.put("requestId", requestId);
                send(exchange, 200, response);
                return;
            }
            if (method.equals("GET") && parts.size() == 1 && parts.get(0).equals("audit")) {
                AuditQuery auditQuery = auditQuery(exchange);
                RuleOpsStore.AuditPage page;
                try {
                    page = store.auditPage(auditQuery.afterSequence(), auditQuery.limit());
                } catch (RuleOpsStore.StoreException e) {
                    throw storeError(e);
                }
                ObjectNode response = base();
                ArrayNode events = response.putArray("events");
                page.events().forEach(event -> events.add(event.asJson(MAPPER)));
                long nextAfterSequence = page.events().isEmpty()
                        ? auditQuery.afterSequence()
                        : page.events().get(page.events().size() - 1).sequence();
                response.put("afterSequence", auditQuery.afterSequence());
                response.put("nextAfterSequence", nextAfterSequence);
                response.put("hasMore", page.hasMore());
                response.put("headDigest", store.auditHeadDigest());
                response.put("requestId", requestId);
                send(exchange, 200, response);
                return;
            }
            if (parts.size() == 1 && parts.get(0).equals("findings")) {
                if (!method.equals("GET")) {
                    send(exchange, 405, error("METHOD_NOT_ALLOWED", "findings is read-only at this API seam", requestId));
                    return;
                }
                ObjectNode response = base();
                response.put("privacyProfile", "digest-only");
                response.putArray("items");
                CollectionQuery collectionQuery = collectionQuery(exchange);
                response.put("cursor", collectionQuery.cursor());
                response.put("nextCursor", collectionQuery.cursor());
                response.put("hasMore", false);
                response.put("requestId", requestId);
                send(exchange, 200, response);
                return;
            }

            if (method.equals("GET")) {
                handleGet(exchange, parts, requestId);
            } else if (method.equals("POST")) {
                handlePost(exchange, parts, requestId);
            } else {
                send(exchange, 405, error("METHOD_NOT_ALLOWED", "only GET and POST are supported", requestId));
            }
        } catch (ApiException e) {
            sendUnchecked(exchange, e.status(), error(e.code(), e.getMessage(), requestId));
        } catch (Exception e) {
            sendUnchecked(exchange, 500, error("INTERNAL_ERROR", "request failed without exposing server details", requestId));
        } finally {
            if (!delegated) {
                exchange.close();
            }
        }
    }

    private boolean dispatchDelivery(HttpExchange exchange, String bundleDigest, String requestId) {
        try {
            deliveryCatalog.recordQueueDepth(deliveryExecutor.getQueue().size());
            deliveryExecutor.execute(() -> {
                try {
                    ruleBundleDelivery(bundleDigest, requestId, exchange);
                    deliveryCatalog.recordResponse(true);
                } catch (ApiException error) {
                    deliveryCatalog.recordResponse(false);
                    sendUnchecked(exchange, error.status(), error(error.code(), error.getMessage(), requestId));
                } catch (IOException error) {
                    deliveryCatalog.recordResponse(false);
                    sendUnchecked(exchange, 503, error("DELIVERY_WRITE_FAILED",
                            "delivery response could not be written", requestId));
                } finally {
                    exchange.close();
                }
            });
            return true;
        } catch (RejectedExecutionException error) {
            deliveryCatalog.recordOverload();
            sendUnchecked(exchange, 503, error("DELIVERY_OVERLOADED",
                    "the bounded delivery executor is temporarily full", requestId));
            return false;
        }
    }

    private void handleGet(HttpExchange exchange, List<String> parts, String requestId)
            throws IOException, ApiException {
        if (parts.size() == 3 && parts.get(0).equals("rule-bundles")
                && parts.get(2).equals("delivery")) {
            ruleBundleDelivery(parts.get(1), requestId, exchange);
            return;
        }
        if (parts.size() == 2 && parts.get(0).equals("submissions") && parts.get(1).equals("status")) {
            submissionStatus(exchange, requestId);
            return;
        }
        RuleOpsStore.ResourceType type = resourceType(parts.get(0));
        if (type == null || parts.size() > 2) {
            throw new ApiException(404, "NOT_FOUND", "API resource was not found");
        }
        ObjectNode response = base();
        if (parts.size() == 1) {
            CollectionQuery collectionQuery = collectionQuery(exchange);
            RuleOpsStore.ResourcePage page;
            try {
                page = store.listPage(type, collectionQuery.cursor(), collectionQuery.limit());
            } catch (RuleOpsStore.StoreException e) {
                throw storeError(e);
            }
            ArrayNode items = response.putArray("items");
            page.resources().forEach(resource -> items.add(resource.asJson(MAPPER)));
            String nextCursor = page.resources().isEmpty()
                    ? collectionQuery.cursor()
                    : RuleOpsStore.resourceCursor(page.resources().get(page.resources().size() - 1));
            response.put("cursor", collectionQuery.cursor());
            response.put("nextCursor", nextCursor);
            response.put("hasMore", page.hasMore());
        } else {
            String version = query(exchange, "version");
            RuleOpsStore.StoredResource resource;
            try {
                resource = store.find(type, parts.get(1), version)
                        .orElseThrow(() -> new ApiException(404, "NOT_FOUND", "resource was not found"));
            } catch (RuleOpsStore.StoreException e) {
                throw storeError(e);
            }
            response.set("resource", resource.asJson(MAPPER));
        }
        response.put("requestId", requestId);
        send(exchange, 200, response, MAX_COLLECTION_RESPONSE_BYTES);
    }

    private void ruleBundleDelivery(String bundleDigest, String requestId, HttpExchange exchange)
            throws IOException, ApiException {
        if (!RuleBundleDeliveryCatalog.isDigest(bundleDigest)) {
            throw new ApiException(400, "BUNDLE_INVALID",
                    "bundle digest must be a lowercase SHA-256 digest");
        }
        RuleBundleDeliveryCatalog.DeliveryRecord record;
        try {
            record = deliveryCatalog.get(bundleDigest)
                    .orElseThrow(() -> new ApiException(404, "BUNDLE_NOT_FOUND",
                            "the requested RuleBundle is not published"));
        } catch (RuleBundleDeliveryCatalog.DeliveryException error) {
            throw deliveryError(error);
        }
        ObjectNode response = base();
        response.setAll(record.asJson(MAPPER));
        response.put("requestId", requestId);
        send(exchange, 200, response);
    }

    private void handlePost(HttpExchange exchange, List<String> parts, String requestId)
            throws IOException, ApiException {
        if (parts.size() == 2 && parts.get(0).equals("coverage") && parts.get(1).equals("analyze")) {
            create(exchange, RuleOpsStore.ResourceType.COVERAGE, null, requestId, null);
            return;
        }
        if (parts.size() == 3 && parts.get(0).equals("rules")
                && (parts.get(2).equals("generate") || parts.get(2).equals("evaluations"))) {
            RuleOpsStore.ResourceType type = parts.get(2).equals("generate")
                    ? RuleOpsStore.ResourceType.FACTORY_JOB : RuleOpsStore.ResourceType.VERIFICATION_JOB;
            create(exchange, type, parts.get(1), requestId, parts.get(1));
            return;
        }
        if (parts.size() == 3 && isLifecycleResource(parts.get(0))
                && (parts.get(2).equals("promote") || parts.get(2).equals("revoke"))) {
            transition(exchange, resourceType(parts.get(0)), parts.get(1),
                    parts.get(2).equals("promote") ? "ACTIVE" : "REVOKED", requestId);
            return;
        }
        if (parts.size() != 1) {
            throw new ApiException(404, "NOT_FOUND", "API resource was not found");
        }
        RuleOpsStore.ResourceType type = resourceType(parts.get(0));
        if (type == null || type == RuleOpsStore.ResourceType.FINDING) {
            throw new ApiException(404, "NOT_FOUND", "API resource is not writable");
        }
        create(exchange, type, null, requestId, null);
    }

    private void create(HttpExchange exchange, RuleOpsStore.ResourceType type, String pathId,
            String requestId, String requiredRuleId) throws IOException, ApiException {
        Headers headers = requiredHeaders(exchange);
        JsonNode body = readJson(exchange);
        Envelope envelope = envelope(body);
        if (type == RuleOpsStore.ResourceType.FEEDBACK || type == RuleOpsStore.ResourceType.EXCEPTION) {
            validateGovernanceRequest(envelope, type, headers);
        }
        if (pathId != null && requiredRuleId != null) {
            JsonNode bodyRuleId = envelope.payload().get("ruleId");
            if (bodyRuleId == null || !pathId.equals(bodyRuleId.asText())) {
                throw new ApiException(400, "PATH_ID_MISMATCH", "payload.ruleId must match the URL rule identity");
            }
        }
        String id = pathId == null ? envelope.id() : envelope.id();
        RuleOpsStore.MutationResult result;
        boolean submission = isSubmissionType(type);
        if (submission) {
            pendingSubmissions.put(envelope.id(), new PendingSubmission(envelope.id(),
                    envelope.payload().get("digest").asText()));
        }
        try {
            result = store.create(type, id, envelope.version(), envelope.payload(),
                    headers.idempotencyKey(), headers.actor(), headers.reason());
        } catch (RuleOpsStore.StoreException e) {
            throw storeError(e);
        } finally {
            if (submission) {
                pendingSubmissions.remove(envelope.id());
            }
        }
        if (submission && result.replayed()) {
            replayedSubmissions.add(envelope.id());
        }
        ObjectNode response = base();
        response.set("resource", result.resource().asJson(MAPPER));
        response.put("replayed", result.replayed());
        response.put("requestId", requestId);
        send(exchange, result.replayed() ? 200 : 201, response);
    }

    private static void validateGovernanceRequest(Envelope envelope, RuleOpsStore.ResourceType type,
            Headers headers) throws ApiException {
        JsonNode payload = envelope.payload();
        if (!payload.isObject()) {
            throw new ApiException(400, "GOVERNANCE_REQUEST_INVALID", "governance request payload must be an object");
        }
        rejectForbiddenFields(payload, "payload");
        Set<String> expectedFields = type == RuleOpsStore.ResourceType.FEEDBACK
                ? Set.of("apiVersion", "kind", "schemaVersion", "status", "requestId", "findingDigest",
                        "findingFingerprint", "ruleId", "classification", "reason", "actor", "digest")
                : Set.of("apiVersion", "kind", "schemaVersion", "status", "requestId", "findingDigest",
                        "findingFingerprint", "ruleId", "scope", "reason", "actor", "digest");
        Set<String> actualFields = new HashSet<>();
        payload.fieldNames().forEachRemaining(actualFields::add);
        if (!actualFields.equals(expectedFields)) {
            throw new ApiException(400, "GOVERNANCE_REQUEST_INVALID",
                    "governance request payload must match its closed digest-only schema");
        }
        String expectedKind = type == RuleOpsStore.ResourceType.FEEDBACK ? "FeedbackDraft" : "ExceptionRequest";
        String expectedVersion = type == RuleOpsStore.ResourceType.FEEDBACK ? "feedback-draft-v1" : "exception-request-v1";
        String expectedStatus = type == RuleOpsStore.ResourceType.FEEDBACK ? "PROPOSED" : "REQUESTED";
        if (!API_VERSION.equals(requiredText(payload, "apiVersion"))
                || !expectedKind.equals(requiredText(payload, "kind"))
                || !expectedVersion.equals(requiredText(payload, "schemaVersion"))
                || !expectedStatus.equals(requiredText(payload, "status"))) {
            throw new ApiException(400, "GOVERNANCE_REQUEST_INVALID", "governance request version or status is invalid");
        }
        String requestId = requiredText(payload, "requestId");
        if (!requestId.equals(envelope.id()) || !requestId.equals(headers.idempotencyKey())) {
            throw new ApiException(400, "REQUEST_ID_MISMATCH", "requestId, envelope id and Idempotency-Key must match");
        }
        requireDigest(payload, "requestId");
        requireDigest(payload, "findingDigest");
        requireDigest(payload, "digest");
        if (!requestId.equals(requestEnvelopeDigest(payload))) {
            throw new ApiException(400, "REQUEST_ID_MISMATCH",
                    "requestId does not match the canonical request envelope");
        }
        requiredText(payload, "findingFingerprint");
        requiredText(payload, "ruleId");
        String actor = requiredContext(payload, "actor", 200);
        String reason = requiredContext(payload, "reason", 1000);
        if (!actor.equals(headers.actor()) || !reason.equals(headers.reason())) {
            throw new ApiException(400, "AUDIT_CONTEXT_MISMATCH", "payload actor/reason must match audit headers");
        }
        if (type == RuleOpsStore.ResourceType.FEEDBACK) {
            String classification = requiredText(payload, "classification");
            if (!Set.of("true-fp", "not-fp", "scope-issue", "documentation").contains(classification)) {
                throw new ApiException(400, "GOVERNANCE_REQUEST_INVALID", "feedback classification is invalid");
            }
        } else if (!validRelativeGlob(requiredText(payload, "scope"))) {
            throw new ApiException(400, "GOVERNANCE_REQUEST_INVALID", "exception scope must be repository-relative");
        }
    }

    private static String requiredText(JsonNode object, String field) throws ApiException {
        JsonNode value = object.get(field);
        if (value == null || !value.isTextual() || value.asText().isBlank()
                || value.asText().chars().anyMatch(Character::isISOControl)) {
            throw new ApiException(400, "GOVERNANCE_REQUEST_INVALID", field + " must be non-empty text");
        }
        return value.asText();
    }

    private static String requiredContext(JsonNode object, String field, int maxLength) throws ApiException {
        String value = requiredText(object, field);
        if (value.length() > maxLength) {
            throw new ApiException(400, "GOVERNANCE_REQUEST_INVALID", field + " is too long");
        }
        return value;
    }

    private static void requireDigest(JsonNode object, String field) throws ApiException {
        String value = requiredText(object, field);
        if (!value.matches("sha256:[0-9a-f]{64}")) {
            throw new ApiException(400, "GOVERNANCE_REQUEST_INVALID", field + " must be a lowercase SHA-256 digest");
        }
        if (field.equals("digest") && !value.equals(contentDigestWithoutSelfField(object))) {
            throw new ApiException(400, "DIGEST_MISMATCH", "digest does not match the canonical request content");
        }
    }

    private static String contentDigestWithoutSelfField(JsonNode object) {
        return canonicalDigestOmitting(object, "digest");
    }

    // requestId 冻结算法（#121 / ADR-0165）：对去除派生自字段 requestId 与 digest 后的
    // 版本化闭合 canonical request envelope 计算 SHA-256，与 Rust 客户端独立复算并交叉校验。
    private static String requestEnvelopeDigest(JsonNode object) {
        return canonicalDigestOmitting(object, "requestId", "digest");
    }

    private static String canonicalDigestOmitting(JsonNode object, String... fields) {
        ObjectNode content = (ObjectNode) object.deepCopy();
        for (String field : fields) {
            content.remove(field);
        }
        byte[] bytes = canonical(content).toString().getBytes(StandardCharsets.UTF_8);
        try {
            return "sha256:" + java.util.HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
        } catch (NoSuchAlgorithmException error) {
            throw new AssertionError("JDK must provide SHA-256", error);
        }
    }

    private static JsonNode canonical(JsonNode node) {
        if (node.isValueNode()) {
            return node.deepCopy();
        }
        if (node.isArray()) {
            ArrayNode result = MAPPER.createArrayNode();
            node.forEach(child -> result.add(canonical(child)));
            return result;
        }
        ObjectNode result = MAPPER.createObjectNode();
        List<String> names = new ArrayList<>();
        node.fieldNames().forEachRemaining(names::add);
        names.sort(String::compareTo);
        names.forEach(name -> result.set(name, canonical(node.get(name))));
        return result;
    }

    private static boolean validRelativeGlob(String value) {
        if (value.isBlank() || value.startsWith("/") || value.contains("\\") || value.contains(":")
                || value.equals("*") || value.equals("**")) {
            return false;
        }
        for (String segment : value.split("/", -1)) {
            if (segment.isEmpty() || segment.equals(".") || segment.equals("..")) {
                return false;
            }
        }
        return true;
    }

    private static void rejectForbiddenFields(JsonNode node, String path) throws ApiException {
        if (node.isObject()) {
            var fields = node.fields();
            while (fields.hasNext()) {
                var field = fields.next();
                if (FORBIDDEN_PAYLOAD_FIELDS.contains(field.getKey())) {
                    throw new ApiException(400, "PRIVACY_FIELD_FORBIDDEN", path + "." + field.getKey()
                            + " is not accepted by the default privacy profile");
                }
                rejectForbiddenFields(field.getValue(), path + "." + field.getKey());
            }
        } else if (node.isArray()) {
            for (int index = 0; index < node.size(); index++) {
                rejectForbiddenFields(node.get(index), path + "[" + index + "]");
            }
        }
    }

    private void transition(HttpExchange exchange, RuleOpsStore.ResourceType type, String id,
            String targetState, String requestId) throws IOException, ApiException {
        Headers headers = requiredHeaders(exchange);
        JsonNode body = readJson(exchange);
        if (!body.isObject() || body.size() > 1 || (body.size() == 1 && !body.has("version"))
                || (body.has("version") && !body.get("version").isTextual())) {
            throw new ApiException(400, "TRANSITION_BODY_INVALID",
                    "lifecycle transition body must contain only a textual version");
        }
        String version = body.has("version") ? body.get("version").asText() : null;
        RuleOpsStore.MutationResult result;
        try {
            result = store.transition(type, id, version, targetState,
                    headers.idempotencyKey(), headers.actor(), headers.reason());
        } catch (RuleOpsStore.StoreException e) {
            throw storeError(e);
        }
        ObjectNode response = base();
        response.set("resource", result.resource().asJson(MAPPER));
        response.put("replayed", result.replayed());
        response.put("requestId", requestId);
        send(exchange, 200, response);
    }

    private void submissionStatus(HttpExchange exchange, String serverRequestId)
            throws IOException, ApiException {
        String requestId = submissionQuery(exchange, "requestId");
        String artifactDigest = submissionQuery(exchange, "artifactDigest");
        validateSubmissionSelector("requestId", requestId);
        validateSubmissionSelector("artifactDigest", artifactDigest);
        if (requestId == null && artifactDigest == null) {
            throw new ApiException(400, "SUBMISSION_SELECTOR_REQUIRED",
                    "requestId or artifactDigest is required");
        }

        PendingSubmission pending = pendingSubmission(requestId, artifactDigest);
        String status;
        String resolvedRequestId = requestId;
        String resolvedArtifactDigest = artifactDigest;
        RuleOpsStore.SubmissionLookup submission = null;
        if (pending != null) {
            resolvedRequestId = pending.requestId();
            resolvedArtifactDigest = pending.artifactDigest();
            status = "PENDING";
        } else {
            java.util.Optional<RuleOpsStore.SubmissionLookup> lookup;
            try {
                lookup = store.findSubmission(requestId, artifactDigest);
            } catch (RuleOpsStore.StoreException e) {
                throw storeError(e);
            }
            submission = lookup.orElse(null);
            if (submission != null) {
                resolvedRequestId = submission.resource().id();
                resolvedArtifactDigest = submission.artifactDigest();
                status = replayedSubmissions.contains(resolvedRequestId) ? "REPLAYED" : "RECEIVED_ONLY";
            } else {
                status = "UNKNOWN";
            }
        }

        ObjectNode response = base();
        response.put("kind", "SubmissionStatus");
        response.put("schemaVersion", "submission-status-v1");
        response.put("status", status);
        putNullable(response, "requestId", resolvedRequestId);
        putNullable(response, "artifactDigest", resolvedArtifactDigest);
        response.put("serverRequestId", serverRequestId);
        response.put("replayed", status.equals("REPLAYED"));
        response.put("automaticRetry", false);
        response.put("authority", status.equals("UNKNOWN") || status.equals("PENDING")
                ? "UNKNOWN" : "RECEIVED_ONLY");
        if (submission != null) {
            response.set("resource", submission.resource().asSubmissionStatusJson(MAPPER));
            ObjectNode audit = response.putObject("auditContext");
            audit.put("operation", submission.audit().operation());
            audit.put("operationId", submission.audit().operationId());
            audit.put("actor", submission.audit().actor());
            audit.put("reason", submission.audit().reason());
            audit.put("resourceType", submission.audit().resourceType().wireName());
            audit.put("resourceId", submission.audit().resourceId());
            audit.put("resourceVersion", submission.audit().resourceVersion());
            audit.put("eventDigest", submission.audit().eventDigest());
            audit.put("recordedAt", submission.audit().recordedAt());
        } else {
            response.putNull("resource");
            response.putNull("auditContext");
        }
        send(exchange, 200, response);
    }

    private PendingSubmission pendingSubmission(String requestId, String artifactDigest) throws ApiException {
        PendingSubmission byRequest = requestId == null ? null : pendingSubmissions.get(requestId);
        List<PendingSubmission> artifactMatches = artifactDigest == null ? List.of() : pendingSubmissions.values().stream()
                    .filter(candidate -> candidate.artifactDigest().equals(artifactDigest))
                    .toList();
        if (artifactMatches.size() > 1) {
            throw new ApiException(409, "SUBMISSION_IDENTITY_CONFLICT",
                    "artifact digest identifies multiple in-flight submissions");
        }
        PendingSubmission byArtifact = artifactMatches.isEmpty() ? null : artifactMatches.get(0);
        if (requestId != null && artifactDigest != null
                && ((byRequest != null && !byRequest.artifactDigest().equals(artifactDigest))
                || (byArtifact != null && !byArtifact.requestId().equals(requestId))
                || (byRequest == null && byArtifact != null)
                || (byRequest != null && byArtifact == null))) {
            throw new ApiException(409, "SUBMISSION_IDENTITY_CONFLICT",
                    "request id and artifact digest identify different submissions");
        }
        return byRequest != null ? byRequest : byArtifact;
    }

    private static void validateSubmissionSelector(String name, String value) throws ApiException {
        if (value != null && !value.matches("sha256:[0-9a-f]{64}")) {
            throw new ApiException(400, "SUBMISSION_SELECTOR_INVALID",
                    name + " must be a lowercase SHA-256 digest");
        }
    }

    private static String submissionQuery(HttpExchange exchange, String name) throws ApiException {
        String raw = exchange.getRequestURI().getRawQuery();
        if (raw == null) {
            return null;
        }
        String value = null;
        boolean found = false;
        for (String pair : raw.split("&", -1)) {
            if (pair.isEmpty()) {
                continue;
            }
            String[] pieces = pair.split("=", 2);
            String key;
            try {
                key = URLDecoder.decode(pieces[0], StandardCharsets.UTF_8);
            } catch (IllegalArgumentException error) {
                throw new ApiException(400, "SUBMISSION_SELECTOR_INVALID",
                        "submission query encoding is invalid");
            }
            if (!name.equals(key)) {
                if (!key.equals("requestId") && !key.equals("artifactDigest")) {
                    throw new ApiException(400, "SUBMISSION_SELECTOR_INVALID",
                            "unknown submission query selector: " + key);
                }
                continue;
            }
            if (pieces.length != 2 || found) {
                throw new ApiException(400, "SUBMISSION_SELECTOR_INVALID",
                        name + " must appear exactly once");
            }
            try {
                value = URLDecoder.decode(pieces[1], StandardCharsets.UTF_8);
            } catch (IllegalArgumentException error) {
                throw new ApiException(400, "SUBMISSION_SELECTOR_INVALID",
                        name + " encoding is invalid");
            }
            found = true;
        }
        return value;
    }

    private static void putNullable(ObjectNode object, String name, String value) {
        if (value == null) {
            object.putNull(name);
        } else {
            object.put(name, value);
        }
    }

    private Headers requiredHeaders(HttpExchange exchange) throws ApiException {
        return new Headers(
                requireHeader(exchange, "Idempotency-Key"),
                requireHeader(exchange, "X-Actor"),
                requireHeader(exchange, "X-Reason"));
    }

    private JsonNode readJson(HttpExchange exchange) throws IOException, ApiException {
        String contentType = exchange.getRequestHeaders().getFirst("Content-Type");
        if (contentType == null || !contentType.toLowerCase().startsWith("application/json")) {
            throw new ApiException(415, "CONTENT_TYPE_INVALID", "Content-Type must be application/json");
        }
        String length = exchange.getRequestHeaders().getFirst("Content-Length");
        if (length != null) {
            try {
                if (Long.parseLong(length) > MAX_BODY_BYTES) {
                    throw new ApiException(413, "REQUEST_TOO_LARGE", "request body exceeds the API limit");
                }
            } catch (NumberFormatException e) {
                throw new ApiException(400, "CONTENT_LENGTH_INVALID", "Content-Length is invalid");
            }
        }
        byte[] bytes = readLimited(exchange.getRequestBody(), MAX_BODY_BYTES);
        try {
            JsonNode node = MAPPER.readTree(bytes);
            if (node == null) {
                throw new ApiException(400, "JSON_INVALID", "request body is empty");
            }
            return node;
        } catch (JsonProcessingException e) {
            throw new ApiException(400, "JSON_INVALID", "request body is not valid JSON");
        }
    }

    private static byte[] readLimited(InputStream input, int max) throws IOException, ApiException {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        byte[] buffer = new byte[8192];
        int total = 0;
        int read;
        while ((read = input.read(buffer)) != -1) {
            total += read;
            if (total > max) {
                throw new ApiException(413, "REQUEST_TOO_LARGE", "request body exceeds the API limit");
            }
            output.write(buffer, 0, read);
        }
        return output.toByteArray();
    }

    private Envelope envelope(JsonNode body) throws ApiException {
        if (!body.isObject() || body.size() != 3
                || !body.has("id") || !body.has("version") || !body.has("payload")) {
            throw new ApiException(400, "ENVELOPE_INVALID", "body must contain only id, version and payload");
        }
        String id = body.get("id").isTextual() ? body.get("id").asText() : "";
        String version = body.get("version").isTextual() ? body.get("version").asText() : "";
        if (id.isBlank() || version.isBlank() || !body.get("payload").isObject()) {
            throw new ApiException(400, "ENVELOPE_INVALID", "id/version must be text and payload must be an object");
        }
        return new Envelope(id, version, body.get("payload"));
    }

    private static RuleOpsStore.ResourceType resourceType(String segment) {
        for (RuleOpsStore.ResourceType type : RuleOpsStore.ResourceType.values()) {
            if (type.wireName().equals(segment)) {
                return type;
            }
        }
        return null;
    }

    private static boolean isLifecycleResource(String segment) {
        return segment.equals("releases") || segment.equals("policies") || segment.equals("exceptions");
    }

    private static boolean isSubmissionType(RuleOpsStore.ResourceType type) {
        return type == RuleOpsStore.ResourceType.FEEDBACK || type == RuleOpsStore.ResourceType.EXCEPTION;
    }

    private static List<String> pathParts(String path) {
        String trimmed = path == null ? "" : path.replaceAll("^/+|/+$", "");
        return trimmed.isBlank() ? List.of() : List.of(trimmed.split("/", -1));
    }

    private static String query(HttpExchange exchange, String name) {
        String raw = exchange.getRequestURI().getRawQuery();
        if (raw == null) {
            return null;
        }
        for (String pair : raw.split("&")) {
            String[] pieces = pair.split("=", 2);
            if (pieces.length == 2 && pieces[0].equals(name)) {
                try {
                    return URLDecoder.decode(pieces[1], StandardCharsets.UTF_8);
                } catch (IllegalArgumentException ignored) {
                    return null;
                }
            }
        }
        return null;
    }

    private static AuditQuery auditQuery(HttpExchange exchange) throws ApiException {
        long afterSequence = 0;
        int limit = RuleOpsStore.MAX_AUDIT_PAGE_SIZE;
        boolean afterFound = false;
        boolean limitFound = false;
        String raw = exchange.getRequestURI().getRawQuery();
        if (raw == null || raw.isEmpty()) {
            return new AuditQuery(afterSequence, limit);
        }
        for (String pair : raw.split("&", -1)) {
            if (pair.isEmpty()) {
                continue;
            }
            String[] pieces = pair.split("=", 2);
            if (pieces.length != 2) {
                throw new ApiException(400, "AUDIT_QUERY_INVALID",
                        "audit query parameters must use key=value form");
            }
            String key = decodeAuditQueryPart(pieces[0], "audit query name");
            String value = decodeAuditQueryPart(pieces[1], key);
            try {
                if (key.equals("afterSequence")) {
                    if (afterFound) {
                        throw new ApiException(400, "AUDIT_QUERY_INVALID",
                                "afterSequence must appear at most once");
                    }
                    afterSequence = Long.parseLong(value);
                    afterFound = true;
                } else if (key.equals("limit")) {
                    if (limitFound) {
                        throw new ApiException(400, "AUDIT_QUERY_INVALID",
                                "limit must appear at most once");
                    }
                    limit = Integer.parseInt(value);
                    limitFound = true;
                } else {
                    throw new ApiException(400, "AUDIT_QUERY_INVALID",
                            "unknown audit query parameter: " + key);
                }
            } catch (NumberFormatException e) {
                throw new ApiException(400, "AUDIT_QUERY_INVALID",
                        key + " must be a decimal integer");
            }
        }
        if (afterSequence < 0 || limit < 1 || limit > RuleOpsStore.MAX_AUDIT_PAGE_SIZE) {
            throw new ApiException(400, "AUDIT_QUERY_INVALID",
                    "afterSequence must be non-negative and limit must be between 1 and "
                            + RuleOpsStore.MAX_AUDIT_PAGE_SIZE);
        }
        return new AuditQuery(afterSequence, limit);
    }

    private static CollectionQuery collectionQuery(HttpExchange exchange) throws ApiException {
        String cursor = "";
        int limit = RuleOpsStore.MAX_RESOURCE_PAGE_SIZE;
        boolean cursorFound = false;
        boolean limitFound = false;
        String raw = exchange.getRequestURI().getRawQuery();
        if (raw == null || raw.isEmpty()) {
            return new CollectionQuery(cursor, limit);
        }
        for (String pair : raw.split("&", -1)) {
            if (pair.isEmpty()) {
                continue;
            }
            String[] pieces = pair.split("=", 2);
            if (pieces.length != 2) {
                throw new ApiException(400, "RESOURCE_QUERY_INVALID",
                        "collection query parameters must use key=value form");
            }
            String key = decodeAuditQueryPart(pieces[0], "collection query name");
            String value = decodeAuditQueryPart(pieces[1], key);
            if (key.equals("cursor")) {
                if (cursorFound) {
                    throw new ApiException(400, "RESOURCE_QUERY_INVALID", "cursor must appear at most once");
                }
                cursor = value;
                cursorFound = true;
            } else if (key.equals("limit")) {
                if (limitFound) {
                    throw new ApiException(400, "RESOURCE_QUERY_INVALID", "limit must appear at most once");
                }
                try {
                    limit = Integer.parseInt(value);
                } catch (NumberFormatException error) {
                    throw new ApiException(400, "RESOURCE_QUERY_INVALID", "limit must be a decimal integer");
                }
                limitFound = true;
            } else {
                throw new ApiException(400, "RESOURCE_QUERY_INVALID", "unknown collection query parameter: " + key);
            }
        }
        if (limit < 1 || limit > RuleOpsStore.MAX_RESOURCE_PAGE_SIZE) {
            throw new ApiException(400, "RESOURCE_QUERY_INVALID",
                    "limit must be between 1 and " + RuleOpsStore.MAX_RESOURCE_PAGE_SIZE);
        }
        return new CollectionQuery(cursor, limit);
    }

    private static String decodeAuditQueryPart(String value, String field) throws ApiException {
        try {
            return URLDecoder.decode(value, StandardCharsets.UTF_8);
        } catch (IllegalArgumentException e) {
            throw new ApiException(400, "AUDIT_QUERY_INVALID", field + " encoding is invalid");
        }
    }

    private static java.util.Optional<String> header(HttpExchange exchange, String name) {
        return java.util.Optional.ofNullable(exchange.getRequestHeaders().getFirst(name))
                .filter(value -> !value.isBlank());
    }

    private static String requireHeader(HttpExchange exchange, String name) throws ApiException {
        return header(exchange, name).orElseThrow(() -> new ApiException(400,
                "HEADER_MISSING", name + " is required for mutating requests"));
    }

    private static ApiException storeError(RuleOpsStore.StoreException error) {
        int status = switch (error.code()) {
            case "NOT_FOUND" -> 404;
            case "IDEMPOTENCY_CONFLICT", "IMMUTABLE_CONFLICT", "STATE_TRANSITION_INVALID",
                    "RESOURCE_VERSION_REQUIRED", "SUBMISSION_IDENTITY_CONFLICT" -> 409;
            case "AUDIT_STATE_INVALID" -> 500;
            case "AUDIT_CAPACITY_EXCEEDED", "OPERATION_CAPACITY_EXCEEDED" -> 503;
            case "AUDIT_QUERY_INVALID", "RESOURCE_QUERY_INVALID" -> 400;
            case "RESOURCE_QUERY_TOO_LARGE" -> 413;
            default -> 400;
        };
        return new ApiException(status, error.code(), error.getMessage());
    }

    private static ApiException deliveryError(RuleBundleDeliveryCatalog.DeliveryException error) {
        int status = switch (error.code()) {
            case "BUNDLE_NOT_FOUND" -> 404;
            case "BUNDLE_IDENTITY_CONFLICT" -> 409;
            case "BUNDLE_CAPACITY_EXCEEDED" -> 503;
            default -> 400;
        };
        return new ApiException(status, error.code(), error.getMessage());
    }

    private static ObjectNode base() {
        ObjectNode response = MAPPER.createObjectNode();
        response.put("apiVersion", API_VERSION);
        return response;
    }

    private static ObjectNode error(String code, String message, String requestId) {
        ObjectNode response = base();
        response.put("code", code);
        response.put("message", message);
        response.put("requestId", requestId);
        return response;
    }

    private static void send(HttpExchange exchange, int status, JsonNode value) throws IOException {
        send(exchange, status, value, RuleBundleDeliveryCatalog.MAX_RESPONSE_BYTES);
    }

    private static void send(HttpExchange exchange, int status, JsonNode value, int maxResponseBytes)
            throws IOException {
        byte[] body = MAPPER.writeValueAsBytes(value);
        if (body.length > maxResponseBytes) {
            throw new IOException("response exceeds the delivery response limit");
        }
        exchange.getResponseHeaders().set("Content-Type", "application/json");
        exchange.getResponseHeaders().set("Cache-Control", "no-store");
        exchange.sendResponseHeaders(status, body.length);
        exchange.getResponseBody().write(body);
    }

    private static void sendUnchecked(HttpExchange exchange, int status, JsonNode value) {
        try {
            send(exchange, status, value);
        } catch (IOException ignored) {
            exchange.close();
        }
    }

    private record Envelope(String id, String version, JsonNode payload) {}

    private record Headers(String idempotencyKey, String actor, String reason) {}

    private record PendingSubmission(String requestId, String artifactDigest) {}

    private record AuditQuery(long afterSequence, int limit) {}

    private record CollectionQuery(String cursor, int limit) {}

    private static final class ApiException extends Exception {
        private final int status;
        private final String code;

        private ApiException(int status, String code, String message) {
            super(message);
            this.status = status;
            this.code = code;
        }

        int status() {
            return status;
        }

        String code() {
            return code;
        }
    }
}
