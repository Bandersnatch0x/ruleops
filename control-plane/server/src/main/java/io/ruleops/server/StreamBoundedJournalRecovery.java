package io.ruleops.server;

import com.fasterxml.jackson.core.JsonParser;
import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.BufferedInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.NoSuchFileException;
import java.nio.file.Path;
import java.nio.file.attribute.BasicFileAttributes;
import java.util.Objects;

/** Strict, bounded JSON-lines recovery for the local RuleOps journal. */
final class StreamBoundedJournalRecovery {
    static final int MAX_ENTRY_BYTES = 4 * 1024 * 1024;
    static final long MAX_JOURNAL_BYTES = 4L * 1024 * 1024 * 1024;

    private static final ObjectMapper MAPPER = new ObjectMapper();
    private final Path path;

    StreamBoundedJournalRecovery(Path path) {
        this.path = path;
    }

    void stream(RuleOpsJournal.EntryConsumer consumer) throws IOException {
        Objects.requireNonNull(consumer, "consumer");
        BasicFileAttributes attributes;
        try {
            attributes = Files.readAttributes(path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS);
        } catch (NoSuchFileException e) {
            return;
        }
        if (!attributes.isRegularFile()) {
            throw new IOException("journal path is not a regular file");
        }
        if (attributes.size() > MAX_JOURNAL_BYTES) {
            throw new IOException("journal exceeds the configured size limit");
        }

        long totalBytes = 0;
        boolean sawBytes = false;
        ByteArrayOutputStream entryBytes = new ByteArrayOutputStream(Math.min(MAX_ENTRY_BYTES, 8192));
        // Keep the no-follow guarantee at the open boundary as well as during
        // the preflight stat. A path replacement between those operations must
        // not turn recovery into an implicit symlink reader.
        try (InputStream input = new BufferedInputStream(Files.newInputStream(path, LinkOption.NOFOLLOW_LINKS))) {
            int next;
            while ((next = input.read()) != -1) {
                sawBytes = true;
                totalBytes++;
                if (totalBytes > MAX_JOURNAL_BYTES) {
                    throw new IOException("journal exceeds the configured size limit");
                }
                if (next == '\n') {
                    if (entryBytes.size() == 0) {
                        throw new IOException("journal contains an empty entry");
                    }
                    consumer.accept(parse(decodeStrict(entryBytes.toByteArray())));
                    entryBytes.reset();
                } else {
                    if (entryBytes.size() >= MAX_ENTRY_BYTES) {
                        throw new IOException("journal entry exceeds the configured size limit");
                    }
                    entryBytes.write(next);
                }
            }
        }
        if (!sawBytes) {
            return;
        }
        if (entryBytes.size() != 0) {
            throw new IOException("journal has a truncated final entry");
        }
    }

    private static String decodeStrict(byte[] bytes) throws IOException {
        try {
            return StandardCharsets.UTF_8.newDecoder()
                    .onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT)
                    .decode(ByteBuffer.wrap(bytes))
                    .toString();
        } catch (CharacterCodingException e) {
            throw new IOException("journal entry is not valid UTF-8", e);
        }
    }

    private static RuleOpsJournal.Entry parse(String line) throws IOException {
        try (JsonParser parser = MAPPER.getFactory().createParser(line)) {
            JsonNode json = MAPPER.readTree(parser);
            if (json == null || !json.isObject()) {
                throw new IOException("journal entry must be a JSON object");
            }
            if (parser.nextToken() != null) {
                throw new IOException("journal entry contains trailing JSON");
            }
            return new RuleOpsJournal.Entry(
                    requiredText(json, "eventType"),
                    requiredLong(json, "sequence"),
                    requiredText(json, "operationId"),
                    requiredText(json, "requestDigest"),
                    requiredText(json, "actor"),
                    requiredText(json, "reason"),
                    resourceType(requiredText(json, "resourceType")),
                    requiredText(json, "resourceId"),
                    requiredText(json, "resourceVersion"),
                    nullableText(json, "fromState"),
                    requiredText(json, "state"),
                    requiredText(json, "resourceDigest"),
                    json.get("payload"),
                    requiredText(json, "previousDigest"),
                    requiredText(json, "eventDigest"),
                    requiredText(json, "recordedAt"));
        } catch (JsonProcessingException | IllegalArgumentException e) {
            throw new IOException("journal entry is malformed", e);
        }
    }

    private static String requiredText(JsonNode json, String field) throws IOException {
        JsonNode value = json.get(field);
        if (value == null || !value.isTextual() || value.asText().isBlank()) {
            throw new IOException("journal field is missing or invalid: " + field);
        }
        return value.asText();
    }

    private static String nullableText(JsonNode json, String field) throws IOException {
        JsonNode value = json.get(field);
        if (value == null || value.isNull()) {
            return null;
        }
        if (!value.isTextual()) {
            throw new IOException("journal field is invalid: " + field);
        }
        return value.asText();
    }

    private static long requiredLong(JsonNode json, String field) throws IOException {
        JsonNode value = json.get(field);
        if (value == null || !value.canConvertToLong()) {
            throw new IOException("journal field is missing or invalid: " + field);
        }
        return value.asLong();
    }

    private static RuleOpsStore.ResourceType resourceType(String value) {
        for (RuleOpsStore.ResourceType type : RuleOpsStore.ResourceType.values()) {
            if (type.wireName().equals(value)) {
                return type;
            }
        }
        throw new IllegalArgumentException("unknown resource type: " + value);
    }
}
