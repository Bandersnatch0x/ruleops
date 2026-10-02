package io.ruleops.server;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.io.IOException;
import java.util.List;
import java.util.Objects;

/** Persistence port for append-only, replayable governance mutations. */
public interface RuleOpsJournal extends AutoCloseable {

    /** Compatibility materialization is intentionally much smaller than the
     * bounded on-disk recovery limit. Callers should prefer {@link #stream}. */
    int MAX_READ_ENTRIES = 4096;
    long MAX_READ_BYTES = 64L * 1024 * 1024;

    @FunctionalInterface
    interface EntryConsumer {
        void accept(Entry entry) throws IOException;
    }

    /**
     * Bounded compatibility snapshot. New recovery paths must use stream so a
     * large journal cannot be materialized as one in-memory collection.
     */
    List<Entry> read() throws IOException;

    default void stream(EntryConsumer consumer) throws IOException {
        Objects.requireNonNull(consumer, "consumer");
        for (Entry entry : read()) {
            consumer.accept(entry);
        }
    }

    void append(Entry entry) throws IOException;

    @Override
    default void close() throws IOException {}

    static RuleOpsJournal noop() {
        return new RuleOpsJournal() {
            @Override
            public List<Entry> read() {
                return List.of();
            }

            @Override
            public void append(Entry entry) {}
        };
    }

    record Entry(
            String eventType,
            long sequence,
            String operationId,
            String requestDigest,
            String actor,
            String reason,
            RuleOpsStore.ResourceType resourceType,
            String resourceId,
            String resourceVersion,
            String fromState,
            String state,
            String resourceDigest,
            JsonNode payload,
            String previousDigest,
            String eventDigest,
            String recordedAt) {

        public Entry {
            payload = payload == null ? null : payload.deepCopy();
        }

        ObjectNode asJson(ObjectMapper mapper) {
            ObjectNode json = mapper.createObjectNode();
            json.put("eventType", eventType);
            json.put("sequence", sequence);
            json.put("operationId", operationId);
            json.put("requestDigest", requestDigest);
            json.put("actor", actor);
            json.put("reason", reason);
            json.put("resourceType", resourceType.wireName());
            json.put("resourceId", resourceId);
            json.put("resourceVersion", resourceVersion);
            if (fromState == null) {
                json.putNull("fromState");
            } else {
                json.put("fromState", fromState);
            }
            json.put("state", state);
            json.put("resourceDigest", resourceDigest);
            json.set("payload", payload == null ? mapper.createObjectNode() : payload.deepCopy());
            json.put("previousDigest", previousDigest);
            json.put("eventDigest", eventDigest);
            json.put("recordedAt", recordedAt);
            return json;
        }
    }
}
