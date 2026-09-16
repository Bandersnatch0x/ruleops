package io.ruleops.javahost;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Path;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HashMap;
import java.util.Map;

/**
 * Maps PMD's line/column diagnostics onto the contract's authoritative
 * SOURCE_SPAN byte coordinates: start_byte/end_byte bracket the violation's
 * first line, and blob_sha256 binds the exact analyzed file content
 * (ADR-0041). Line/column remain as derived diagnostic fields.
 */
final class SourceSpans {

    private static final Map<String, byte[]> DIGEST_CACHE = new HashMap<>();
    private static final Map<String, long[]> LINE_INDEX_CACHE = new HashMap<>();

    private SourceSpans() {}

    record Span(byte[] blobSha256, long startByte, long endByte, int line, int column) {}

    static Span forViolation(Path absoluteFile, String relativePath, int line, int column)
            throws IOException {
        byte[] digest = digestOf(absoluteFile);
        long[] lineOffsets = lineIndex(absoluteFile);
        int safeLine = Math.max(1, line);
        if (safeLine > lineOffsets.length - 1) {
            // Out-of-range coordinates: emit an empty span at EOF; the runtime
            // rejects coordinates it cannot verify.
            long end = lineOffsets[lineOffsets.length - 1];
            return new Span(digest, end, end, line, column);
        }
        long startByte = lineOffsets[safeLine - 1];
        long endByte = lineOffsets[safeLine];
        return new Span(digest, startByte, endByte, line, column);
    }

    static byte[] digestOf(Path file) throws IOException {
        synchronized (DIGEST_CACHE) {
            byte[] cached = DIGEST_CACHE.get(file.toString());
            if (cached != null) {
                return cached;
            }
        }
        MessageDigest md;
        try {
            md = MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
        byte[] digest;
        try (InputStream in = java.nio.file.Files.newInputStream(file)) {
            byte[] buf = new byte[8192];
            int n;
            while ((n = in.read(buf)) > 0) {
                md.update(buf, 0, n);
            }
            digest = md.digest();
        }
        synchronized (DIGEST_CACHE) {
            DIGEST_CACHE.put(file.toString(), digest);
        }
        return digest;
    }

    /** Offsets of every line start, plus one trailing EOF offset. */
    private static long[] lineIndex(Path file) throws IOException {
        synchronized (LINE_INDEX_CACHE) {
            long[] cached = LINE_INDEX_CACHE.get(file.toString());
            if (cached != null) {
                return cached;
            }
        }
        byte[] bytes = java.nio.file.Files.readAllBytes(file);
        java.util.List<Long> starts = new java.util.ArrayList<>();
        starts.add(0L);
        for (int i = 0; i < bytes.length; i++) {
            if (bytes[i] == '\n') {
                starts.add((long) (i + 1));
            }
        }
        starts.add((long) bytes.length);
        long[] offsets = new long[starts.size()];
        for (int i = 0; i < starts.size(); i++) {
            offsets[i] = starts.get(i);
        }
        synchronized (LINE_INDEX_CACHE) {
            LINE_INDEX_CACHE.put(file.toString(), offsets);
        }
        return offsets;
    }

    /** Debug helper used in tests. */
    static String hex(byte[] bytes) {
        StringBuilder sb = new StringBuilder();
        for (byte b : bytes) {
            sb.append(String.format("%02x", b));
        }
        return sb.toString();
    }

    static byte[] utf8(String s) {
        return s.getBytes(StandardCharsets.UTF_8);
    }
}
