package io.ruleops.javahost;

import erf.backend.v0.AnalysisUnit;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.List;

/**
 * Validates and materializes the digest-bound Java build context transported
 * by an ATTESTED_BUILD_EXPORT WorkspaceDescriptor. SAFE_STATIC descriptors
 * leave this context empty; an adapter never guesses a classpath or build
 * output from the local filesystem.
 */
final class JavaExecutionContext {

    private static final byte[] CONTEXT_VERSION =
            "ruleops-java-context-v1".getBytes(StandardCharsets.UTF_8);

    private final List<Path> classpathEntries;
    private final List<Path> buildOutputRoots;
    private final String invocationIdentity;

    private JavaExecutionContext(
            List<Path> classpathEntries,
            List<Path> buildOutputRoots,
            String invocationIdentity) {
        this.classpathEntries = List.copyOf(classpathEntries);
        this.buildOutputRoots = List.copyOf(buildOutputRoots);
        this.invocationIdentity = invocationIdentity;
    }

    static JavaExecutionContext from(
            Path repoRoot, AnalysisUnit unit) throws IOException {
        boolean hasContext = !unit.getClasspathEntriesList().isEmpty()
                || !unit.getBuildOutputRootsList().isEmpty()
                || !unit.getInvocationIdentity().isEmpty()
                || !unit.getClasspathSha256().isEmpty()
                || !unit.getBuildOutputSha256().isEmpty();
        if (!hasContext) {
            return new JavaExecutionContext(List.of(), List.of(), "");
        }
        if (unit.getInvocationIdentity().isEmpty()) {
            throw new IOException(
                    "digest-bound Java execution context is missing invocation identity");
        }
        if (unit.getClasspathSha256().size() != 32
                || unit.getBuildOutputSha256().size() != 32) {
            throw new IOException(
                    "digest-bound Java execution context is missing a classpath or build-output digest");
        }
        List<Path> classpath = resolveEntries(
                repoRoot, unit.getClasspathEntriesList(), "classpath");
        List<Path> outputs = resolveEntries(
                repoRoot, unit.getBuildOutputRootsList(), "build output");
        verifyDigest(
                "classpath",
                unit.getClasspathEntriesList(),
                unit.getClasspathSha256().toByteArray());
        verifyDigest(
                "build output",
                unit.getBuildOutputRootsList(),
                unit.getBuildOutputSha256().toByteArray());
        return new JavaExecutionContext(classpath, outputs,
                unit.getInvocationIdentity());
    }

    List<Path> classpathEntries() {
        return classpathEntries;
    }

    List<Path> buildOutputRoots() {
        return buildOutputRoots;
    }

    List<Path> classpathAndBuildOutputs() {
        List<Path> entries = new ArrayList<>(classpathEntries);
        entries.addAll(buildOutputRoots);
        return List.copyOf(entries);
    }

    String invocationIdentity() {
        return invocationIdentity;
    }

    private static List<Path> resolveEntries(
            Path repoRoot, List<String> rawEntries, String kind)
            throws IOException {
        List<Path> resolved = new ArrayList<>();
        for (String raw : rawEntries) {
            if (raw.isEmpty() || raw.indexOf('\0') >= 0) {
                throw new IOException(kind + " entry is empty or contains NUL");
            }
            final Path requested;
            try {
                requested = Path.of(raw);
            } catch (RuntimeException error) {
                throw new IOException("invalid " + kind + " entry: " + raw, error);
            }
            Path candidate = requested.isAbsolute()
                    ? requested.toAbsolutePath().normalize()
                    : repoRoot.resolve(requested).toAbsolutePath().normalize();
            if (!candidate.startsWith(repoRoot)
                    || containsParentSegment(requested)) {
                throw new IOException(
                        kind + " entry escapes the repository snapshot: " + raw);
            }
            ensureRealPath(repoRoot, candidate, kind, raw);
            if (kind.equals("build output")
                    && !Files.isDirectory(candidate, LinkOption.NOFOLLOW_LINKS)) {
                throw new IOException("build output root is not a directory: " + raw);
            }
            resolved.add(candidate);
        }
        return resolved;
    }

    private static void ensureRealPath(
            Path repoRoot, Path candidate, String kind, String raw)
            throws IOException {
        if (!Files.exists(candidate, LinkOption.NOFOLLOW_LINKS)
                || Files.isSymbolicLink(candidate)) {
            throw new IOException(kind + " entry is missing or symbolic: " + raw);
        }
        Path cursor = repoRoot;
        for (Path component : repoRoot.relativize(candidate)) {
            cursor = cursor.resolve(component);
            if (Files.isSymbolicLink(cursor)) {
                throw new IOException(
                        kind + " entry traverses a symbolic link: " + raw);
            }
        }
    }

    private static boolean containsParentSegment(Path path) {
        for (Path component : path) {
            if (component.toString().equals("..")) {
                return true;
            }
        }
        return false;
    }

    private static void verifyDigest(
            String label, List<String> entries, byte[] supplied)
            throws IOException {
        if (supplied.length != 32) {
            throw new IOException(label + " digest must be exactly 32 bytes");
        }
        byte[] expected = digest(label, entries);
        if (!MessageDigest.isEqual(expected, supplied)) {
            throw new IOException(label + " digest does not match transported entries");
        }
    }

    static byte[] digest(String label, List<String> entries)
            throws IOException {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            ByteArrayOutputStream bytes = new ByteArrayOutputStream();
            DataOutputStream output = new DataOutputStream(bytes);
            output.write(CONTEXT_VERSION);
            output.writeByte(0);
            output.write(label.getBytes(StandardCharsets.UTF_8));
            output.writeByte(0);
            for (String entry : entries) {
                byte[] encoded = entry.getBytes(StandardCharsets.UTF_8);
                output.writeLong(encoded.length);
                output.write(encoded);
            }
            output.flush();
            return digest.digest(bytes.toByteArray());
        } catch (NoSuchAlgorithmException error) {
            throw new IOException("SHA-256 is unavailable", error);
        }
    }
}
