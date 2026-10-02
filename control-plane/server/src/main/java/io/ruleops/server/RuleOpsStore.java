package io.ruleops.server;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.TreeMap;

/**
 * V0.2 domain port backed by an in-memory adapter and an optional journal.
 *
 * <p>The adapter deliberately owns only API seam semantics: immutable resource
 * identity, idempotent writes, lifecycle transitions, privacy rejection and a
 * chained audit stream. The journal port allows restart/recovery without
 * changing the HTTP contract.</p>
 */
public final class RuleOpsStore {

    public enum ResourceType {
        STANDARD("standards"),
        COVERAGE("coverage"),
        RULE("rules"),
        FACTORY_JOB("factory-jobs"),
        VERIFICATION_JOB("verification-jobs"),
        RELEASE("releases"),
        POLICY("policies"),
        SCAN("scans"),
        FINDING("findings"),
        FEEDBACK("feedback"),
        EXCEPTION("exceptions");

        private final String wireName;

        ResourceType(String wireName) {
            this.wireName = wireName;
        }

        public String wireName() {
            return wireName;
        }
    }

    public record StoredResource(
            ResourceType type,
            String id,
            String version,
            String state,
            String digest,
            JsonNode payload) {

        public StoredResource {
            payload = payload == null ? null : payload.deepCopy();
        }

        ObjectNode asJson(ObjectMapper mapper) {
            ObjectNode result = mapper.createObjectNode();
            result.put("type", type.wireName());
            result.put("id", id);
            result.put("version", version);
            result.put("state", state);
            result.put("digest", digest);
            result.set("payload", payload == null ? mapper.createObjectNode() : payload.deepCopy());
            return result;
        }

        ObjectNode asSubmissionStatusJson(ObjectMapper mapper) {
            ObjectNode result = mapper.createObjectNode();
            result.put("type", type.wireName());
            result.put("id", id);
            result.put("version", version);
            result.put("state", state);
            result.put("digest", digest);
            return result;
        }
    }

    public record AuditEvent(
            long sequence,
            String operation,
            String operationId,
            String actor,
            String reason,
            ResourceType resourceType,
            String resourceId,
            String resourceVersion,
            String resourceDigest,
            String previousDigest,
            String eventDigest,
            String recordedAt) {

        ObjectNode asJson(ObjectMapper mapper) {
            ObjectNode result = mapper.createObjectNode();
            result.put("sequence", sequence);
            result.put("operation", operation);
            result.put("operationId", operationId);
            result.put("actor", actor);
            result.put("reason", reason);
            result.put("resourceType", resourceType.wireName());
            result.put("resourceId", resourceId);
            result.put("resourceVersion", resourceVersion);
            result.put("resourceDigest", resourceDigest);
            result.put("previousDigest", previousDigest);
            result.put("eventDigest", eventDigest);
            result.put("recordedAt", recordedAt);
            return result;
        }
    }

    public record MutationResult(StoredResource resource, boolean replayed) {}

    /** Read-only identity and audit projection for a feedback/exception submission. */
    public record SubmissionLookup(StoredResource resource, AuditEvent audit, String artifactDigest) {}

    /** Immutable, version-qualified lifecycle mutation request. */
    public record VersionQualifiedLifecycleTransition(
            ResourceType type,
            String id,
            String version,
            String targetState) {}

    public static final class StoreException extends Exception {
        private final String code;

        public StoreException(String code, String message) {
            super(message);
            this.code = code;
        }

        public String code() {
            return code;
        }
    }

    public static final int MAX_AUDIT_EVENTS = 100_000;
    public static final int MAX_AUDIT_PAGE_SIZE = 256;
    public static final int MAX_RESOURCE_PAGE_SIZE = 256;
    public static final int MAX_OPERATION_RECORDS = MAX_AUDIT_EVENTS;
    public static final int MAX_RESOURCE_PAGE_BYTES = 4 * 1024 * 1024 - 4096;

    public record AuditPage(List<AuditEvent> events, boolean hasMore) {
        public AuditPage {
            events = List.copyOf(events);
        }
    }

    public record ResourcePage(List<StoredResource> resources, boolean hasMore) {
        public ResourcePage {
            resources = List.copyOf(resources);
        }
    }

    private static final class RecoveryFailure extends RuntimeException {
        private final StoreException failure;

        private RecoveryFailure(StoreException failure) {
            super(failure);
            this.failure = failure;
        }
    }

    private record Key(ResourceType type, String id, String version) {}

    private record Operation(
            String requestDigest,
            Key key,
            String operation,
            String fromState,
            String targetState,
            StoredResource result) {}

    private static final Set<String> FORBIDDEN_PAYLOAD_FIELDS = Set.of(
            "source", "sourceCode", "sourceContent", "sourceSnippet", "fullFinding", "findings");

    private final ObjectMapper mapper = new ObjectMapper();
    private final Map<Key, StoredResource> resources = new TreeMap<>(Comparator
            .comparing((Key key) -> key.type().wireName())
            .thenComparing(Key::id)
            .thenComparing(Key::version));
    private final Map<String, Operation> operations = new HashMap<>();
    private final List<AuditEvent> audit = new ArrayList<>();
    private final RuleOpsJournal journal;
    private long nextAuditSequence = 1;

    public RuleOpsStore() {
        this.journal = RuleOpsJournal.noop();
    }

    public RuleOpsStore(RuleOpsJournal journal) throws StoreException {
        this.journal = journal == null ? RuleOpsJournal.noop() : journal;
        try {
            this.journal.stream(entry -> {
                try {
                    recoverEntry(entry);
                } catch (StoreException e) {
                    throw new RecoveryFailure(e);
                }
            });
        } catch (RecoveryFailure e) {
            throw e.failure;
        } catch (java.io.IOException e) {
            throw new StoreException("JOURNAL_CORRUPT", "cannot recover RuleOps journal: " + e.getMessage());
        }
    }

    public synchronized MutationResult create(
            ResourceType type,
            String id,
            String version,
            JsonNode payload,
            String operationId,
            String actor,
            String reason) throws StoreException {
        requireIdentity(type, id, version);
        requireContext(operationId, actor, reason);
        if (payload == null || !payload.isObject()) {
            throw new StoreException("PAYLOAD_INVALID", "payload must be a JSON object");
        }
        rejectForbiddenFields(payload, "payload");
        Key key = new Key(type, id, version);
        String requestDigest = digest(envelope(type, id, version, payload));
        Operation previous = operations.get(operationId);
        if (previous != null) {
            if (!previous.requestDigest().equals(requestDigest)) {
                throw new StoreException("IDEMPOTENCY_CONFLICT", "operation id was reused with a different request");
            }
            StoredResource replay = previous.result();
            if (replay == null) {
                throw new StoreException("AUDIT_STATE_INVALID", "idempotent operation has no stored resource");
            }
            return new MutationResult(replay, true);
        }

        StoredResource existing = resources.get(key);
        if (existing != null) {
            if (!existing.digest().equals(requestDigest)) {
                throw new StoreException("IMMUTABLE_CONFLICT", "resource identity already has different content");
            }
            // A duplicate immutable create needs no replay ledger entry: the
            // resource identity and digest already make the result stable.
            // Avoid allowing arbitrary idempotency keys to grow the ledger.
            return new MutationResult(existing, true);
        }

        StoredResource created = new StoredResource(type, id, version, initialState(type), requestDigest,
                canonical(payload));
        AuditEvent event = buildAudit("CREATE", operationId, actor, reason, created);
        ensureAuditCapacity();
        ensureOperationCapacity(operationId);
        appendJournal(new RuleOpsJournal.Entry(
                "CREATE", event.sequence(), operationId, requestDigest, actor, reason, type, id, version,
                null, created.state(), created.digest(), created.payload(), event.previousDigest(),
                event.eventDigest(), event.recordedAt()));
        resources.put(key, created);
        recordOperation(operationId, new Operation(requestDigest, key, "CREATE", null, null, created));
        commitAudit(event);
        return new MutationResult(created, false);
    }

    public synchronized MutationResult transition(
            ResourceType type,
            String id,
            String version,
            String targetState,
            String operationId,
            String actor,
            String reason) throws StoreException {
        return transition(new VersionQualifiedLifecycleTransition(type, id, version, targetState),
                operationId, actor, reason);
    }

    public synchronized MutationResult transition(
            VersionQualifiedLifecycleTransition transitionRequest,
            String operationId,
            String actor,
            String reason) throws StoreException {
        if (transitionRequest == null) {
            throw new StoreException("IDENTITY_INVALID", "lifecycle transition request is required");
        }
        ResourceType type = transitionRequest.type();
        String id = transitionRequest.id();
        String version = transitionRequest.version();
        String targetState = transitionRequest.targetState();
        requireTransitionIdentity(type, id, version);
        requireContext(operationId, actor, reason);
        if (targetState == null || targetState.isBlank() || targetState.length() > 32
                || targetState.chars().anyMatch(Character::isWhitespace)) {
            throw new StoreException("STATE_INVALID", "target state must be a compact non-empty value");
        }
        Key key = new Key(type, id, version);
        StoredResource current = resources.get(key);
        if (current == null) {
            throw new StoreException("NOT_FOUND", "resource was not found");
        }
        ObjectNode request = transitionRequest(type, key, current.state(), targetState);
        String requestDigest = digest(request);
        Operation previous = operations.get(operationId);
        if (previous != null) {
            if (!previous.key().equals(key) || !previous.operation().equals("TRANSITION")
                    || !targetState.equals(previous.targetState())
                    || !previous.requestDigest().equals(
                            digest(transitionRequest(type, key, previous.fromState(), targetState)))) {
                throw new StoreException("IDEMPOTENCY_CONFLICT", "operation id was reused with a different request");
            }
            return new MutationResult(previous.result(), true);
        }
        if (!allowedTransition(type, current.state(), targetState)) {
            throw new StoreException("STATE_TRANSITION_INVALID",
                    "cannot transition " + type.wireName() + " from " + current.state() + " to " + targetState);
        }
        if (current.state().equals(targetState)) {
            recordOperation(operationId, new Operation(requestDigest, key, "TRANSITION", current.state(),
                    targetState, current));
            return new MutationResult(current, true);
        }
        StoredResource updated = new StoredResource(
                current.type(), current.id(), current.version(), targetState, current.digest(), current.payload());
        AuditEvent event = buildAudit("TRANSITION_" + targetState, operationId, actor, reason, updated);
        ensureAuditCapacity();
        ensureOperationCapacity(operationId);
        appendJournal(new RuleOpsJournal.Entry(
                "TRANSITION", event.sequence(), operationId, requestDigest, actor, reason, type, id,
                key.version(), current.state(), targetState, updated.digest(), updated.payload(),
                event.previousDigest(), event.eventDigest(), event.recordedAt()));
        resources.put(key, updated);
        recordOperation(operationId, new Operation(requestDigest, key, "TRANSITION", current.state(),
                targetState, updated));
        commitAudit(event);
        return new MutationResult(updated, false);
    }

    public synchronized Optional<StoredResource> find(ResourceType type, String id, String version)
            throws StoreException {
        if (type == null || id == null || id.isBlank()) {
            throw new StoreException("IDENTITY_INVALID", "resource type and id are required");
        }
        List<StoredResource> matches = resources.entrySet().stream()
                .filter(entry -> entry.getKey().type() == type
                        && entry.getKey().id().equals(id)
                        && (version == null || entry.getKey().version().equals(version)))
                .map(Map.Entry::getValue)
                .toList();
        if (version == null && matches.size() > 1) {
            throw new StoreException("RESOURCE_VERSION_REQUIRED", "resource id has multiple immutable versions");
        }
        return matches.stream().findFirst();
    }

    public synchronized List<StoredResource> list(ResourceType type) {
        return resources.values().stream()
                .filter(resource -> resource.type() == type)
                .toList();
    }

    /** Stable cursor page over the resource collection. */
    public synchronized ResourcePage listPage(ResourceType type, String afterCursor, int limit)
            throws StoreException {
        if (type == null) {
            throw new StoreException("RESOURCE_QUERY_INVALID", "resource type is required");
        }
        if (limit < 1 || limit > MAX_RESOURCE_PAGE_SIZE) {
            throw new StoreException("RESOURCE_QUERY_INVALID",
                    "limit must be between 1 and " + MAX_RESOURCE_PAGE_SIZE);
        }
        String cursor = afterCursor == null ? "" : afterCursor;
        if (cursor.indexOf('\n') >= 0 || cursor.indexOf('\r') >= 0) {
            throw new StoreException("RESOURCE_QUERY_INVALID", "cursor contains a line break");
        }
        List<StoredResource> page = new ArrayList<>();
        int pageBytes = 0;
        boolean hasMore = false;
        for (StoredResource resource : resources.values()) {
            if (resource.type() != type || (!cursor.isEmpty() && resourceCursor(resource).compareTo(cursor) <= 0)) {
                continue;
            }
            if (page.size() == limit) {
                hasMore = true;
                break;
            }
            int resourceBytes = resource.asJson(mapper).toString().getBytes(StandardCharsets.UTF_8).length;
            if (resourceBytes > MAX_RESOURCE_PAGE_BYTES) {
                throw new StoreException("RESOURCE_QUERY_TOO_LARGE",
                        "a resource exceeds the collection response byte budget");
            }
            if (pageBytes > MAX_RESOURCE_PAGE_BYTES - resourceBytes) {
                hasMore = true;
                break;
            }
            page.add(resource);
            pageBytes += resourceBytes;
        }
        return new ResourcePage(page, hasMore);
    }

    public static String resourceCursor(StoredResource resource) {
        return resource.id() + "\u0000" + resource.version();
    }

    /**
     * Resolve a previously submitted digest-only governance request without
     * mutating resources, replaying a write, or returning source content.
     */
    public synchronized Optional<SubmissionLookup> findSubmission(String requestId, String artifactDigest)
            throws StoreException {
        if ((requestId == null || requestId.isBlank())
                && (artifactDigest == null || artifactDigest.isBlank())) {
            throw new StoreException("SUBMISSION_SELECTOR_REQUIRED",
                    "request id or artifact digest is required");
        }
        List<StoredResource> requestMatches = resources.values().stream()
                .filter(RuleOpsStore::isSubmission)
                .filter(resource -> requestId != null && !requestId.isBlank() && resource.id().equals(requestId))
                .toList();
        List<StoredResource> artifactMatches = resources.values().stream()
                .filter(RuleOpsStore::isSubmission)
                .filter(resource -> artifactDigest != null && !artifactDigest.isBlank()
                        && artifactDigest.equals(submissionArtifactDigest(resource)))
                .toList();
        if (requestMatches.size() > 1 || artifactMatches.size() > 1) {
            throw new StoreException("SUBMISSION_IDENTITY_CONFLICT",
                    "a submission selector identifies multiple immutable resources");
        }
        Optional<StoredResource> byRequest = requestMatches.stream().findFirst();
        Optional<StoredResource> byArtifact = artifactMatches.stream().findFirst();
        if (requestId != null && !requestId.isBlank() && artifactDigest != null && !artifactDigest.isBlank()) {
            if (byRequest.isPresent() && !artifactDigest.equals(submissionArtifactDigest(byRequest.get()))) {
                throw new StoreException("SUBMISSION_IDENTITY_CONFLICT",
                        "request id and artifact digest identify different submissions");
            }
            if (byArtifact.isPresent() && !requestId.equals(byArtifact.get().id())) {
                throw new StoreException("SUBMISSION_IDENTITY_CONFLICT",
                        "request id and artifact digest identify different submissions");
            }
        }
        StoredResource resource = byRequest.or(() -> byArtifact).orElse(null);
        if (resource == null) {
            return Optional.empty();
        }
        AuditEvent createAudit = audit.stream()
                .filter(event -> event.operation().equals("CREATE"))
                .filter(event -> event.resourceType() == resource.type())
                .filter(event -> event.resourceId().equals(resource.id()))
                .filter(event -> event.resourceVersion().equals(resource.version()))
                .findFirst()
                .orElseThrow(() -> new StoreException("AUDIT_STATE_INVALID",
                        "submission resource has no create audit event"));
        return Optional.of(new SubmissionLookup(resource, createAudit, submissionArtifactDigest(resource)));
    }

    public synchronized List<AuditEvent> audit() {
        return List.copyOf(audit);
    }

    public synchronized AuditPage auditPage(long afterSequence, int limit) throws StoreException {
        if (afterSequence < 0) {
            throw new StoreException("AUDIT_QUERY_INVALID", "afterSequence must be non-negative");
        }
        if (limit < 1 || limit > MAX_AUDIT_PAGE_SIZE) {
            throw new StoreException("AUDIT_QUERY_INVALID",
                    "limit must be between 1 and " + MAX_AUDIT_PAGE_SIZE);
        }
        List<AuditEvent> page = new ArrayList<>(limit);
        boolean hasMore = false;
        for (AuditEvent event : audit) {
            if (event.sequence() <= afterSequence) {
                continue;
            }
            if (page.size() == limit) {
                hasMore = true;
                break;
            }
            page.add(event);
        }
        return new AuditPage(page, hasMore);
    }

    public synchronized String auditHeadDigest() {
        return audit.isEmpty() ? "sha256:" + "0".repeat(64) : audit.get(audit.size() - 1).eventDigest();
    }

    private AuditEvent buildAudit(String operation, String operationId, String actor, String reason,
            StoredResource resource) {
        String previous = auditHeadDigest();
        String recordedAt = Instant.now().toString();
        AuditEvent draft = new AuditEvent(nextAuditSequence, operation, operationId, actor, reason,
                resource.type(), resource.id(), resource.version(), resource.digest(), previous,
                "", recordedAt);
        return new AuditEvent(nextAuditSequence, operation, operationId, actor, reason,
                resource.type(), resource.id(), resource.version(), resource.digest(), previous,
                auditDigest(draft), recordedAt);
    }

    private void commitAudit(AuditEvent event) {
        audit.add(event);
        nextAuditSequence++;
    }

    private void ensureAuditCapacity() throws StoreException {
        if (audit.size() >= MAX_AUDIT_EVENTS) {
            throw new StoreException("AUDIT_CAPACITY_EXCEEDED",
                    "audit event capacity has been reached; archive and restart the store");
        }
    }

    private void recordOperation(String operationId, Operation operation) throws StoreException {
        ensureOperationCapacity(operationId);
        operations.put(operationId, operation);
    }

    private void ensureOperationCapacity(String operationId) throws StoreException {
        if (!operations.containsKey(operationId) && operations.size() >= MAX_OPERATION_RECORDS) {
            throw new StoreException("OPERATION_CAPACITY_EXCEEDED",
                    "idempotency operation capacity has been reached");
        }
    }

    private void appendJournal(RuleOpsJournal.Entry entry) throws StoreException {
        try {
            journal.append(entry);
        } catch (java.io.IOException e) {
            throw new StoreException("JOURNAL_APPEND_FAILED", "cannot append governance journal: " + e.getMessage());
        }
    }

    private void recoverEntry(RuleOpsJournal.Entry entry) throws StoreException {
            if (entry.sequence() != nextAuditSequence) {
                throw new StoreException("JOURNAL_SEQUENCE_INVALID", "journal sequence is not monotonic");
            }
            if (!entry.previousDigest().equals(auditHeadDigest())) {
                throw new StoreException("JOURNAL_CHAIN_INVALID", "journal audit chain is broken");
            }
            if (entry.payload() == null || !entry.payload().isObject()) {
                throw new StoreException("JOURNAL_ENTRY_INVALID", "journal payload must be an object");
            }
            rejectForbiddenFields(entry.payload(), "journal.payload");
            Key key = new Key(entry.resourceType(), entry.resourceId(), entry.resourceVersion());
            String expectedDigest = digest(envelope(entry.resourceType(), entry.resourceId(),
                    entry.resourceVersion(), entry.payload()));
            if (!expectedDigest.equals(entry.resourceDigest())) {
                throw new StoreException("JOURNAL_DIGEST_INVALID", "journal resource digest does not match payload");
            }
            AuditEvent event = new AuditEvent(entry.sequence(),
                    entry.eventType().equals("CREATE") ? "CREATE" : "TRANSITION_" + entry.state(),
                    entry.operationId(), entry.actor(), entry.reason(), entry.resourceType(),
                    entry.resourceId(), entry.resourceVersion(), entry.resourceDigest(),
                    entry.previousDigest(), entry.eventDigest(), entry.recordedAt());
            if (!entry.eventDigest().equals(auditDigest(event))) {
                throw new StoreException("JOURNAL_DIGEST_INVALID", "journal audit digest does not match event");
            }
            if (operations.containsKey(entry.operationId())) {
                throw new StoreException("JOURNAL_OPERATION_DUPLICATE", "journal operation id is duplicated");
            }
            if (entry.eventType().equals("CREATE")) {
                if (resources.containsKey(key) || !initialState(entry.resourceType()).equals(entry.state())
                        || entry.fromState() != null) {
                    throw new StoreException("JOURNAL_STATE_INVALID", "journal create event is inconsistent");
                }
                StoredResource resource = new StoredResource(entry.resourceType(), entry.resourceId(),
                        entry.resourceVersion(), entry.state(), entry.resourceDigest(), canonical(entry.payload()));
                if (!entry.requestDigest().equals(digest(envelope(entry.resourceType(), entry.resourceId(),
                        entry.resourceVersion(), entry.payload())))) {
                    throw new StoreException("JOURNAL_DIGEST_INVALID", "journal request digest is invalid");
                }
                resources.put(key, resource);
            } else if (entry.eventType().equals("TRANSITION")) {
                StoredResource current = resources.get(key);
                if (current == null || !current.state().equals(entry.fromState())
                        || !current.digest().equals(entry.resourceDigest())
                        || !allowedTransition(entry.resourceType(), entry.fromState(), entry.state())) {
                    throw new StoreException("JOURNAL_STATE_INVALID", "journal transition event is inconsistent");
                }
                ObjectNode request = transitionRequest(entry.resourceType(), key, current.state(), entry.state());
                if (!entry.requestDigest().equals(digest(request))) {
                    throw new StoreException("JOURNAL_DIGEST_INVALID", "journal transition request digest is invalid");
                }
                resources.put(key, new StoredResource(current.type(), current.id(), current.version(),
                        entry.state(), current.digest(), current.payload()));
            } else {
                throw new StoreException("JOURNAL_ENTRY_INVALID", "unknown journal event type");
            }
            StoredResource result = resources.get(key);
            recordOperation(entry.operationId(), new Operation(entry.requestDigest(), key,
                    entry.eventType().equals("CREATE") ? "CREATE" : "TRANSITION",
                    entry.eventType().equals("CREATE") ? null : entry.fromState(),
                    entry.eventType().equals("CREATE") ? null : entry.state(), result));
            ensureAuditCapacity();
            commitAudit(event);
    }

    private ObjectNode transitionRequest(ResourceType type, Key key, String from, String to) {
        ObjectNode request = mapper.createObjectNode();
        request.put("type", type.wireName());
        request.put("id", key.id());
        request.put("version", key.version());
        request.put("from", from);
        request.put("to", to);
        return request;
    }

    private static String auditDigest(AuditEvent event) {
        ObjectMapper mapper = new ObjectMapper();
        ObjectNode json = mapper.createObjectNode();
        json.put("sequence", event.sequence());
        json.put("operation", event.operation());
        json.put("operationId", event.operationId());
        json.put("actor", event.actor());
        json.put("reason", event.reason());
        json.put("resourceType", event.resourceType().wireName());
        json.put("resourceId", event.resourceId());
        json.put("resourceVersion", event.resourceVersion());
        json.put("resourceDigest", event.resourceDigest());
        json.put("previousDigest", event.previousDigest());
        json.put("recordedAt", event.recordedAt());
        try {
            byte[] bytes = json.toString().getBytes(StandardCharsets.UTF_8);
            return "sha256:" + java.util.HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
        } catch (NoSuchAlgorithmException e) {
            throw new AssertionError("JDK must provide SHA-256", e);
        }
    }

    private static String initialState(ResourceType type) {
        return switch (type) {
            case RULE, RELEASE -> "CANDIDATE";
            case POLICY -> "DRAFT";
            case EXCEPTION -> "REQUESTED";
            case FEEDBACK -> "OPEN";
            default -> "ACCEPTED";
        };
    }

    private static boolean isSubmission(StoredResource resource) {
        return resource.type() == ResourceType.FEEDBACK || resource.type() == ResourceType.EXCEPTION;
    }

    private static String submissionArtifactDigest(StoredResource resource) {
        JsonNode digest = resource.payload() == null ? null : resource.payload().get("digest");
        return digest != null && digest.isTextual() ? digest.asText() : "";
    }

    private static boolean allowedTransition(ResourceType type, String from, String to) {
        if (type == ResourceType.RELEASE) {
            return (from.equals("CANDIDATE") && to.equals("ACTIVE"))
                    || ((from.equals("CANDIDATE") || from.equals("ACTIVE")) && to.equals("REVOKED"));
        }
        if (type == ResourceType.POLICY) {
            return (from.equals("DRAFT") && to.equals("ACTIVE"))
                    || ((from.equals("DRAFT") || from.equals("ACTIVE")) && to.equals("REVOKED"));
        }
        if (type == ResourceType.EXCEPTION) {
            return (from.equals("REQUESTED") && to.equals("ACTIVE"))
                    || ((from.equals("REQUESTED") || from.equals("ACTIVE")) && to.equals("REVOKED"));
        }
        return false;
    }

    private static void requireIdentity(ResourceType type, String id, String version) throws StoreException {
        if (type == null || id == null || id.isBlank() || version == null || version.isBlank()) {
            throw new StoreException("IDENTITY_INVALID", "resource type, id and version are required");
        }
        if (id.length() > 200 || version.length() > 100
                || id.chars().anyMatch(Character::isISOControl)
                || version.chars().anyMatch(Character::isISOControl)) {
            throw new StoreException("IDENTITY_INVALID", "resource identity is too long or contains control characters");
        }
    }

    private static void requireTransitionIdentity(ResourceType type, String id, String version)
            throws StoreException {
        if (type == null || id == null || id.isBlank()) {
            throw new StoreException("IDENTITY_INVALID", "resource type and id are required");
        }
        if (version == null || version.isBlank()) {
            throw new StoreException("RESOURCE_VERSION_REQUIRED",
                    "lifecycle transition requires an explicit resource version");
        }
        requireIdentity(type, id, version);
    }

    private static void requireContext(String operationId, String actor, String reason) throws StoreException {
        if (operationId == null || operationId.isBlank() || actor == null || actor.isBlank()
                || reason == null || reason.isBlank()) {
            throw new StoreException("AUDIT_CONTEXT_MISSING", "operation id, actor and reason are required");
        }
        if (operationId.length() > 200 || actor.length() > 200 || reason.length() > 1000
                || operationId.chars().anyMatch(Character::isISOControl)
                || actor.chars().anyMatch(Character::isISOControl)
                || reason.chars().anyMatch(Character::isISOControl)) {
            throw new StoreException("AUDIT_CONTEXT_INVALID", "audit context is too long or contains control characters");
        }
    }

    private static void rejectForbiddenFields(JsonNode node, String path) throws StoreException {
        if (node.isObject()) {
            var fields = node.fields();
            while (fields.hasNext()) {
                var field = fields.next();
                if (FORBIDDEN_PAYLOAD_FIELDS.contains(field.getKey())) {
                    throw new StoreException("PRIVACY_FIELD_FORBIDDEN", path + "." + field.getKey()
                            + " is not accepted by the default privacy profile");
                }
                rejectForbiddenFields(field.getValue(), path + "." + field.getKey());
            }
        } else if (node.isArray()) {
            for (int i = 0; i < node.size(); i++) {
                rejectForbiddenFields(node.get(i), path + "[" + i + "]");
            }
        }
    }

    private ObjectNode envelope(ResourceType type, String id, String version, JsonNode payload) {
        ObjectNode root = mapper.createObjectNode();
        root.put("type", type.wireName());
        root.put("id", id);
        root.put("version", version);
        root.set("payload", canonical(payload));
        return root;
    }

    private JsonNode canonical(JsonNode node) {
        if (node == null || node.isValueNode()) {
            return node == null ? mapper.nullNode() : node.deepCopy();
        }
        if (node.isArray()) {
            ArrayNode result = mapper.createArrayNode();
            node.forEach(child -> result.add(canonical(child)));
            return result;
        }
        ObjectNode result = mapper.createObjectNode();
        var names = new HashSet<String>();
        node.fieldNames().forEachRemaining(names::add);
        names.stream().sorted().forEach(name -> result.set(name, canonical(node.get(name))));
        return result;
    }

    private static String digest(JsonNode node) {
        try {
            byte[] bytes = node.toString().getBytes(StandardCharsets.UTF_8);
            return "sha256:" + java.util.HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
        } catch (NoSuchAlgorithmException e) {
            throw new AssertionError("JDK must provide SHA-256", e);
        }
    }
}
