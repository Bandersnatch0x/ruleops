package io.ruleops.server;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class RuleOpsJournalTest {
    private static final ObjectMapper JSON = new ObjectMapper();

    @TempDir
    Path temp;

    @Test
    void fileJournalRebuildsResourcesAndAuditAfterRestart() throws Exception {
        Path journalPath = temp.resolve("ruleops.journal");
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            RuleOpsStore first = new RuleOpsStore(journal);
            first.create(RuleOpsStore.ResourceType.RELEASE, "release-1", "1.0.0",
                    JSON.readTree("{\"ruleDigest\":\"sha256:r\"}"),
                    "create", "release-owner", "publish candidate");
            first.transition(RuleOpsStore.ResourceType.RELEASE, "release-1", "1.0.0", "ACTIVE",
                    "promote", "governance", "approve release");
            assertEquals(2, first.audit().size());
        }

        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            RuleOpsStore recovered = new RuleOpsStore(journal);
            assertEquals("ACTIVE", recovered.find(RuleOpsStore.ResourceType.RELEASE, "release-1", "1.0.0")
                    .orElseThrow().state());
            assertEquals(2, recovered.audit().size());
            assertEquals(recovered.audit().get(1).eventDigest(), recovered.auditHeadDigest());
            RuleOpsStore.MutationResult replay = recovered.create(RuleOpsStore.ResourceType.RELEASE, "release-1",
                    "1.0.0", JSON.readTree("{\"ruleDigest\":\"sha256:r\"}"),
                    "create", "release-owner", "publish candidate");
            assertTrue(replay.replayed());
        }
    }

    @Test
    void versionQualifiedTransitionReplaysTheSameRevisionAfterRestart() throws Exception {
        Path journalPath = temp.resolve("version-qualified-transition.journal");
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            RuleOpsStore first = new RuleOpsStore(journal);
            first.create(RuleOpsStore.ResourceType.RELEASE, "release-shared", "1.0.0",
                    JSON.readTree("{\"ruleDigest\":\"sha256:v1\"}"),
                    "create-v1", "release-owner", "publish v1");
            first.create(RuleOpsStore.ResourceType.RELEASE, "release-shared", "2.0.0",
                    JSON.readTree("{\"ruleDigest\":\"sha256:v2\"}"),
                    "create-v2", "release-owner", "publish v2");
            RuleOpsStore.MutationResult promoted = first.transition(
                    RuleOpsStore.ResourceType.RELEASE, "release-shared", "2.0.0", "ACTIVE",
                    "promote-v2", "governance", "approve v2");
            assertEquals("2.0.0", promoted.resource().version());
            assertEquals("ACTIVE", promoted.resource().state());
            assertTrue(first.transition(RuleOpsStore.ResourceType.RELEASE, "release-shared", "2.0.0",
                    "ACTIVE", "promote-v2", "governance", "approve v2").replayed());
        }

        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            RuleOpsStore recovered = new RuleOpsStore(journal);
            assertEquals("CANDIDATE", recovered.find(RuleOpsStore.ResourceType.RELEASE, "release-shared", "1.0.0")
                    .orElseThrow().state());
            assertEquals("ACTIVE", recovered.find(RuleOpsStore.ResourceType.RELEASE, "release-shared", "2.0.0")
                    .orElseThrow().state());
            assertEquals(3, recovered.audit().size());
            assertTrue(recovered.transition(RuleOpsStore.ResourceType.RELEASE, "release-shared", "2.0.0",
                    "ACTIVE", "promote-v2", "governance", "approve v2").replayed());
        }
    }

    @Test
    void truncatedJournalFailsClosedInsteadOfSilentlyDroppingTheTail() throws Exception {
        Path journalPath = temp.resolve("corrupt.journal");
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            RuleOpsStore store = new RuleOpsStore(journal);
            store.create(RuleOpsStore.ResourceType.POLICY, "policy-1", "1.0.0",
                    JSON.readTree("{\"profile\":\"java\"}"), "create", "policy-owner", "publish policy");
        }
        byte[] bytes = Files.readAllBytes(journalPath);
        Files.write(journalPath, java.util.Arrays.copyOf(bytes, bytes.length - 1));
        RuleOpsStore.StoreException error;
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            error = assertThrows(RuleOpsStore.StoreException.class, () -> new RuleOpsStore(journal));
        }
        assertEquals("JOURNAL_CORRUPT", error.code());
    }

    @Test
    void invalidUtf8JournalFailsClosedWithoutReplacement() throws Exception {
        Path journalPath = temp.resolve("invalid-utf8.journal");
        Files.write(journalPath, new byte[] {'{', '"', (byte) 0xC3, 0x28, '"', '}', '\n'});

        RuleOpsStore.StoreException error;
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            error = assertThrows(RuleOpsStore.StoreException.class, () -> new RuleOpsStore(journal));
        }

        assertEquals("JOURNAL_CORRUPT", error.code());
        assertTrue(error.getMessage().contains("UTF-8"), error.getMessage());
    }

    @Test
    void oversizedJournalEntryFailsClosedBeforeUnboundedGrowth() throws Exception {
        Path journalPath = temp.resolve("oversized-entry.journal");
        try (FileChannel channel = FileChannel.open(journalPath, StandardOpenOption.CREATE,
                StandardOpenOption.WRITE)) {
            byte[] chunk = new byte[8192];
            Arrays.fill(chunk, (byte) 'x');
            long remaining = 4L * 1024 * 1024 + 1;
            while (remaining > 0) {
                int length = (int) Math.min(remaining, chunk.length);
                channel.write(ByteBuffer.wrap(chunk, 0, length));
                remaining -= length;
            }
            channel.write(ByteBuffer.wrap(new byte[] {'\n'}));
        }

        RuleOpsStore.StoreException error;
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            error = assertThrows(RuleOpsStore.StoreException.class, () -> new RuleOpsStore(journal));
        }

        assertEquals("JOURNAL_CORRUPT", error.code());
        assertTrue(error.getMessage().contains("entry exceeds"), error.getMessage());
    }

    @Test
    void fourGiBJournalLimitIsCheckedWithSparseFixture() throws Exception {
        Path journalPath = temp.resolve("four-gib-limit.journal");
        try (FileChannel channel = FileChannel.open(journalPath, StandardOpenOption.CREATE,
                StandardOpenOption.WRITE)) {
            channel.position(4L * 1024 * 1024 * 1024);
            channel.write(ByteBuffer.wrap(new byte[] {'\n'}));
        }

        RuleOpsStore.StoreException error;
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            error = assertThrows(RuleOpsStore.StoreException.class, () -> new RuleOpsStore(journal));
        }

        assertEquals("JOURNAL_CORRUPT", error.code());
        assertTrue(error.getMessage().contains("configured size limit"), error.getMessage());
    }

    @Test
    void emptyJournalLineFailsClosed() throws Exception {
        Path journalPath = temp.resolve("empty-line.journal");
        Files.write(journalPath, new byte[] {'\n'});

        RuleOpsStore.StoreException error;
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            error = assertThrows(RuleOpsStore.StoreException.class, () -> new RuleOpsStore(journal));
        }

        assertEquals("JOURNAL_CORRUPT", error.code());
        assertTrue(error.getMessage().contains("empty entry"), error.getMessage());
    }

    @Test
    void trailingJsonTokensFailClosed() throws Exception {
        Path journalPath = temp.resolve("trailing-json.journal");
        Files.writeString(journalPath, "{} {}\n");

        RuleOpsStore.StoreException error;
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            error = assertThrows(RuleOpsStore.StoreException.class, () -> new RuleOpsStore(journal));
        }

        assertEquals("JOURNAL_CORRUPT", error.code());
        assertTrue(error.getMessage().contains("trailing JSON"), error.getMessage());
    }

    @Test
    void malformedJsonFailsClosed() throws Exception {
        Path journalPath = temp.resolve("malformed-json.journal");
        Files.writeString(journalPath, "{not-json}\n");

        RuleOpsStore.StoreException error;
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            error = assertThrows(RuleOpsStore.StoreException.class, () -> new RuleOpsStore(journal));
        }

        assertEquals("JOURNAL_CORRUPT", error.code());
        assertTrue(error.getMessage().contains("malformed"), error.getMessage());
    }

    @Test
    void sequenceMismatchFailsClosedDuringStreamingRecovery() throws Exception {
        Path journalPath = journalWithSingleEntry("sequence-mismatch.journal");
        ObjectNode entry = readSingleEntry(journalPath);
        entry.put("sequence", 2);
        writeSingleEntry(journalPath, entry);

        RuleOpsStore.StoreException error = recoverFailure(journalPath);

        assertEquals("JOURNAL_SEQUENCE_INVALID", error.code());
    }

    @Test
    void chainDigestMismatchFailsClosedDuringStreamingRecovery() throws Exception {
        Path journalPath = journalWithSingleEntry("chain-mismatch.journal");
        ObjectNode entry = readSingleEntry(journalPath);
        entry.put("previousDigest", "sha256:" + "1".repeat(64));
        writeSingleEntry(journalPath, entry);

        RuleOpsStore.StoreException error = recoverFailure(journalPath);

        assertEquals("JOURNAL_CHAIN_INVALID", error.code());
    }

    @Test
    void resourceDigestMismatchFailsClosedDuringStreamingRecovery() throws Exception {
        Path journalPath = journalWithSingleEntry("resource-digest-mismatch.journal");
        ObjectNode entry = readSingleEntry(journalPath);
        entry.put("resourceDigest", "sha256:" + "0".repeat(64));
        writeSingleEntry(journalPath, entry);

        RuleOpsStore.StoreException error = recoverFailure(journalPath);

        assertEquals("JOURNAL_DIGEST_INVALID", error.code());
    }

    @Test
    void nonRegularJournalPathFailsClosedInsteadOfBecomingEmpty() throws Exception {
        Path journalPath = temp.resolve("journal-directory");
        Files.createDirectory(journalPath);

        RuleOpsStore.StoreException error = recoverFailure(journalPath);

        assertEquals("JOURNAL_CORRUPT", error.code());
        assertTrue(error.getMessage().contains("regular file"), error.getMessage());
    }

    @Test
    void appendRejectsFourGiBJournalLimit() throws Exception {
        Path journalPath = temp.resolve("append-four-gib-limit.journal");
        try (FileChannel channel = FileChannel.open(journalPath, StandardOpenOption.CREATE,
                StandardOpenOption.WRITE)) {
            channel.position(StreamBoundedJournalRecovery.MAX_JOURNAL_BYTES - 1);
            channel.write(ByteBuffer.wrap(new byte[] {0}));
        }

        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            IOException error = assertThrows(IOException.class, () -> journal.append(sampleEntry()));
            assertTrue(error.getMessage().contains("configured size limit"), error.getMessage());
        }
    }

    @Test
    void appendFailureDoesNotCommitAnInMemoryMutation() throws Exception {
        RuleOpsJournal failing = new RuleOpsJournal() {
            @Override
            public List<Entry> read() {
                return List.of();
            }

            @Override
            public void append(Entry entry) throws IOException {
                throw new IOException("simulated storage failure");
            }
        };
        RuleOpsStore store = new RuleOpsStore(failing);

        RuleOpsStore.StoreException error = assertThrows(RuleOpsStore.StoreException.class,
                () -> store.create(RuleOpsStore.ResourceType.RULE, "rule-1", "1.0.0",
                        JSON.readTree("{\"kind\":\"RuleIntent\"}"), "op-1", "author", "draft rule"));

        assertEquals("JOURNAL_APPEND_FAILED", error.code());
        assertTrue(store.list(RuleOpsStore.ResourceType.RULE).isEmpty());
        assertTrue(store.audit().isEmpty());
    }

    @Test
    void transitionAppendFailureDoesNotCommitTheVersionQualifiedMutation() throws Exception {
        List<RuleOpsJournal.Entry> entries = new ArrayList<>();
        RuleOpsJournal failingTransition = new RuleOpsJournal() {
            @Override
            public List<Entry> read() {
                return List.copyOf(entries);
            }

            @Override
            public void append(Entry entry) throws IOException {
                if (entry.eventType().equals("TRANSITION")) {
                    throw new IOException("simulated transition storage failure");
                }
                entries.add(entry);
            }
        };
        RuleOpsStore store = new RuleOpsStore(failingTransition);
        store.create(RuleOpsStore.ResourceType.RELEASE, "release-append", "1.0.0",
                JSON.readTree("{\"ruleDigest\":\"sha256:append\"}"),
                "append-create", "release-owner", "publish candidate");

        RuleOpsStore.StoreException error = assertThrows(RuleOpsStore.StoreException.class,
                () -> store.transition(RuleOpsStore.ResourceType.RELEASE, "release-append", "1.0.0", "ACTIVE",
                        "append-promote", "governance", "approve release"));

        assertEquals("JOURNAL_APPEND_FAILED", error.code());
        assertEquals("CANDIDATE", store.find(RuleOpsStore.ResourceType.RELEASE, "release-append", "1.0.0")
                .orElseThrow().state());
        assertEquals(1, store.audit().size());
        assertEquals(1, entries.size());
    }

    private Path journalWithSingleEntry(String name) throws Exception {
        Path journalPath = temp.resolve(name);
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            RuleOpsStore store = new RuleOpsStore(journal);
            store.create(RuleOpsStore.ResourceType.POLICY, "policy-1", "1.0.0",
                    JSON.readTree("{\"profile\":\"java\"}"), "create", "policy-owner", "publish policy");
        }
        return journalPath;
    }

    private ObjectNode readSingleEntry(Path journalPath) throws IOException {
        return (ObjectNode) JSON.readTree(Files.readString(journalPath).trim());
    }

    private void writeSingleEntry(Path journalPath, ObjectNode entry) throws IOException {
        Files.writeString(journalPath, entry.toString() + "\n");
    }

    private RuleOpsStore.StoreException recoverFailure(Path journalPath) throws Exception {
        try (FileRuleOpsJournal journal = new FileRuleOpsJournal(journalPath)) {
            return assertThrows(RuleOpsStore.StoreException.class, () -> new RuleOpsStore(journal));
        }
    }

    private RuleOpsJournal.Entry sampleEntry() {
        return new RuleOpsJournal.Entry(
                "CREATE", 1, "operation", "request", "actor", "reason",
                RuleOpsStore.ResourceType.POLICY, "policy-1", "1.0.0", null, "CANDIDATE",
                "resource", JSON.createObjectNode().put("profile", "java"),
                "sha256:" + "0".repeat(64), "sha256:" + "1".repeat(64), "2026-09-19T00:00:00Z");
    }
}
