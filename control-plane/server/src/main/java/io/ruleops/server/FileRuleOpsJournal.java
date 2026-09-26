package io.ruleops.server;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.List;

/** Durable local append-only journal adapter for the Shadow/Pilot server seam. */
public final class FileRuleOpsJournal implements RuleOpsJournal {
    private static final ObjectMapper MAPPER = new ObjectMapper();
    private final Path path;

    public FileRuleOpsJournal(Path path) {
        this.path = path.toAbsolutePath().normalize();
    }

    public Path path() {
        return path;
    }

    @Override
    public List<Entry> read() throws IOException {
        List<Entry> entries = new ArrayList<>();
        final long[] estimatedBytes = {0};
        stream(entry -> {
            if (entries.size() >= RuleOpsJournal.MAX_READ_ENTRIES) {
                throw new IOException("journal compatibility read exceeds the configured entry limit");
            }
            long entryBytes = MAPPER.writeValueAsBytes(entry.asJson(MAPPER)).length;
            if (estimatedBytes[0] > RuleOpsJournal.MAX_READ_BYTES - entryBytes) {
                throw new IOException("journal compatibility read exceeds the configured byte limit");
            }
            estimatedBytes[0] += entryBytes;
            entries.add(entry);
        });
        return List.copyOf(entries);
    }

    @Override
    public void stream(EntryConsumer consumer) throws IOException {
        new StreamBoundedJournalRecovery(path).stream(consumer);
    }

    @Override
    public void append(Entry entry) throws IOException {
        Path parent = path.getParent();
        if (parent != null) {
            Files.createDirectories(parent);
        }
        rejectSymbolicLinks(path);
        byte[] bytes;
        try {
            bytes = (MAPPER.writeValueAsString(entry.asJson(MAPPER)) + "\n")
                    .getBytes(StandardCharsets.UTF_8);
        } catch (JsonProcessingException e) {
            throw new IOException("cannot encode journal entry", e);
        }
        if (bytes.length > StreamBoundedJournalRecovery.MAX_ENTRY_BYTES) {
            throw new IOException("journal entry exceeds the configured size limit");
        }
        try (FileChannel channel = FileChannel.open(path, StandardOpenOption.CREATE,
                StandardOpenOption.WRITE, StandardOpenOption.APPEND, LinkOption.NOFOLLOW_LINKS)) {
            if (channel.size() > StreamBoundedJournalRecovery.MAX_JOURNAL_BYTES - bytes.length) {
                throw new IOException("journal exceeds the configured size limit");
            }
            ByteBuffer buffer = ByteBuffer.wrap(bytes);
            while (buffer.hasRemaining()) {
                channel.write(buffer);
            }
            channel.force(true);
        }
    }

    private static void rejectSymbolicLinks(Path candidate) throws IOException {
        Path absolute = candidate.toAbsolutePath().normalize();
        Path current = absolute.getRoot();
        if (current == null) {
            throw new IOException("journal path has no absolute root");
        }
        for (Path component : absolute) {
            current = current.resolve(component);
            if (Files.isSymbolicLink(current)) {
                throw new IOException("journal path contains a symbolic link: " + current);
            }
        }
    }

}
