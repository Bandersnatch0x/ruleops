package io.ruleops.javahost;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import erf.backend.v0.AnalysisUnit;
import erf.backend.v0.BackendObservation;
import erf.backend.v0.BytecodeMember;
import erf.backend.v0.ObservationLocation;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import javax.tools.JavaCompiler;
import javax.tools.ToolProvider;
import org.junit.jupiter.api.Test;

class SpotBugsBackendAdapterTest {

    @Test
    void compiledRepresentativeProducesTypedBytecodeObservation() throws Exception {
        Path root = Files.createTempDirectory("erf-spotbugs-test-");
        try {
            Path source = root.resolve("src/main/java/Bad.java");
            Files.createDirectories(source.getParent());
            Files.writeString(source, "public class Bad { void run() { System.exit(1); } }\n");
            Path classes = root.resolve("target/classes");
            Files.createDirectories(classes);
            compile(source, classes);

            SpotBugsBackendAdapter adapter = SpotBugsBackendAdapter.load(root);
            AnalysisUnit unit = AnalysisUnit.newBuilder()
                    .setUnitId("spotbugs-representative")
                    .setLanguage("java")
                    .addFiles("target/classes/Bad.class")
                    .build();
            List<BackendObservation> observations = adapter.observe(unit);

            BackendObservation observation = observations.stream()
                    .filter(value -> value.getRuleId().equals("spotbugs.dm-exit"))
                    .findFirst()
                    .orElseThrow(() -> new AssertionError("DM_EXIT observation missing: " + observations));
            assertEquals(ObservationLocation.KindCase.BYTECODE_MEMBER,
                    observation.getLocation().getKindCase());
            BytecodeMember member = observation.getLocation().getBytecodeMember();
            assertEquals("Bad", member.getClassFqn());
            assertTrue(observation.getMessage().contains("DM_EXIT"));
            assertEquals(observations, adapter.observe(unit), "observation order must be stable");
        } finally {
            deleteTree(root);
        }
    }

    @Test
    void missingOrNonClassInputFailsClosed() throws Exception {
        Path root = Files.createTempDirectory("erf-spotbugs-invalid-");
        try {
            SpotBugsBackendAdapter adapter = SpotBugsBackendAdapter.load(root);
            AnalysisUnit missing = AnalysisUnit.newBuilder()
                    .setUnitId("missing")
                    .setLanguage("java")
                    .addFiles("target/classes/Missing.class")
                    .build();
            assertThrows(IOException.class, () -> adapter.observe(missing));

            Path text = root.resolve("target/classes/not-a-class.class");
            Files.createDirectories(text.getParent());
            Files.writeString(text, "not bytecode");
            AnalysisUnit invalid = AnalysisUnit.newBuilder()
                    .setUnitId("invalid")
                    .setLanguage("java")
                    .addFiles("target/classes/not-a-class.class")
                    .build();
            assertThrows(IOException.class, () -> adapter.observe(invalid));
        } finally {
            deleteTree(root);
        }
    }

    private static void compile(Path source, Path output) {
        JavaCompiler compiler = ToolProvider.getSystemJavaCompiler();
        assertTrue(compiler != null, "JDK compiler is required by this test");
        assertEquals(0, compiler.run(null, null, null, "-g", "-d", output.toString(), source.toString()));
    }

    private static void deleteTree(Path root) throws IOException {
        if (root == null) return;
        try (var paths = Files.walk(root)) {
            paths.sorted((a, b) -> b.compareTo(a)).forEach(path -> {
                try {
                    Files.deleteIfExists(path);
                } catch (IOException e) {
                    throw new RuntimeException(e);
                }
            });
        }
    }
}
