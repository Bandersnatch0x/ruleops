package io.ruleops.javahost;

import static org.junit.jupiter.api.Assertions.assertThrows;

import erf.backend.v0.AnalysisUnit;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;

class PmdBackendAdapterTest {

    @Test
    void repositoryFileResolutionRejectsTraversalAndSymlinks() throws Exception {
        Path root = Files.createTempDirectory("erf-pmd-boundary-");
        Path outside = Files.createTempFile("erf-pmd-outside-", ".java");
        try {
            Path ruleset = root.resolve("ruleset.xml");
            Files.writeString(ruleset, """
                    <ruleset name="boundary-test" xmlns="http://pmd.sourceforge.net/ruleset/2.0.0">
                      <description>boundary test</description>
                      <rule ref="category/java/errorprone.xml/EmptyCatchBlock"/>
                    </ruleset>
                    """);
            PmdBackendAdapter adapter = PmdBackendAdapter.load(root, ruleset);

            Path linkedRuleset = root.resolve("linked-ruleset.xml");
            Files.createSymbolicLink(linkedRuleset, ruleset);
            assertThrows(IOException.class, () -> PmdBackendAdapter.load(root, linkedRuleset));

            AnalysisUnit traversal = AnalysisUnit.newBuilder()
                    .setUnitId("traversal")
                    .setLanguage("java")
                    .addFiles("../" + outside.getFileName())
                    .build();
            assertThrows(IOException.class, () -> adapter.observe(traversal));

            Path linked = root.resolve("linked.java");
            Files.createSymbolicLink(linked, outside);
            AnalysisUnit symlink = AnalysisUnit.newBuilder()
                    .setUnitId("symlink")
                    .setLanguage("java")
                    .addFiles("linked.java")
                    .build();
            assertThrows(IOException.class, () -> adapter.observe(symlink));

            Path linkedDirectory = root.resolve("linked-dir");
            Files.createSymbolicLink(linkedDirectory, outside.getParent());
            AnalysisUnit nestedSymlink = AnalysisUnit.newBuilder()
                    .setUnitId("nested-symlink")
                    .setLanguage("java")
                    .addFiles("linked-dir/" + outside.getFileName())
                    .build();
            assertThrows(IOException.class, () -> adapter.observe(nestedSymlink));
        } finally {
            Files.deleteIfExists(root.resolve("linked.java"));
            Files.deleteIfExists(root.resolve("linked-dir"));
            Files.deleteIfExists(root.resolve("linked-ruleset.xml"));
            Files.deleteIfExists(outside);
            deleteTree(root);
        }
    }

    @Test
    void partialAttestedContextIsRejectedInsteadOfGuessed() throws Exception {
        Path root = Files.createTempDirectory("erf-pmd-context-");
        try {
            Path ruleset = root.resolve("ruleset.xml");
            Files.writeString(ruleset, """
                    <ruleset name="context-test" xmlns="http://pmd.sourceforge.net/ruleset/2.0.0">
                      <description>context test</description>
                      <rule ref="category/java/errorprone.xml/EmptyCatchBlock"/>
                    </ruleset>
                    """);
            PmdBackendAdapter adapter = PmdBackendAdapter.load(root, ruleset);
            AnalysisUnit partial = AnalysisUnit.newBuilder()
                    .setUnitId("partial-context")
                    .setLanguage("java")
                    .addFiles("src/main/java/A.java")
                    .setInvocationIdentity("maven@verified")
                    .build();
            assertThrows(IOException.class, () -> adapter.observe(partial));

            byte[] classpathDigest =
                    JavaExecutionContext.digest("classpath", List.of());
            byte[] outputDigest =
                    JavaExecutionContext.digest("build output", List.of());
            AnalysisUnit missingOutput = partial.toBuilder()
                    .setClasspathSha256(
                            com.google.protobuf.ByteString.copyFrom(classpathDigest))
                    .setBuildOutputSha256(
                            com.google.protobuf.ByteString.copyFrom(outputDigest))
                    .build();
            assertThrows(IOException.class, () -> adapter.observe(missingOutput));
        } finally {
            deleteTree(root);
        }
    }

    private static void deleteTree(Path root) throws IOException {
        try (var paths = Files.walk(root)) {
            paths.sorted((a, b) -> b.compareTo(a)).forEach(path -> {
                try {
                    Files.deleteIfExists(path);
                } catch (IOException error) {
                    throw new RuntimeException(error);
                }
            });
        }
    }
}
