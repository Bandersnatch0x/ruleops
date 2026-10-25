package io.ruleops.ontology;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class OntologyContractPackCompilerTest {
    private static Path root(String file) { return Path.of("..", "..", "specs", "ontology", file); }

    @Test
    void sameRevisionBuildsByteIdenticalPack() throws Exception {
        var compiler = new OntologyContractPackCompiler();
        var a = compiler.compile(root("core.ttl"), List.of(root("java-extension.ttl")), "HEAD");
        var b = compiler.compile(root("core.ttl"), List.of(root("java-extension.ttl")), "HEAD");
        assertEquals(a.digest(), b.digest());
        assertArrayEquals(a.canonicalTriples(), b.canonicalTriples());
        assertTrue(a.identity().startsWith("ontology-pack-v0:"));
    }

    /**
     * Cross-language consumers use this checked-in vector as the wire-format
     * contract for the current v0 compiler.  Keep the expected bytes and
     * digest stable even if the source Turtle formatting changes.
     */
    @Test
    void coreJavaVectorMatchesCanonicalBytesAndDigest() throws Exception {
        var pack = new OntologyContractPackCompiler().compile(
                root("core.ttl"), List.of(root("java-extension.ttl")), "rdfc-v0-core-java");
        var expected = Files.readString(root("vectors/core-java-v0.nt"), StandardCharsets.UTF_8);
        assertArrayEquals(expected.getBytes(StandardCharsets.UTF_8), pack.canonicalTriples());
        assertEquals("146149a0cb35d92e012d2b05e1cdbe796a4779f40eddc666fa10353fabe1d905", pack.digest());
        assertEquals("ontology-pack-v0:146149a0cb35d92e012d2b05e1cdbe796a4779f40eddc666fa10353fabe1d905", pack.identity());
    }

    @Test
    void undeclaredIriFailsClosedWorld(@TempDir Path tmp) throws Exception {
        Path bad = tmp.resolve("bad.ttl");
        Files.writeString(bad, "@prefix erf: <https://ruleops.io/ontology/core#> .\n"
                + "erf:Known erf:pointsTo erf:Missing .\n");
        var e = assertThrows(OntologyCompileException.class,
                () -> new OntologyContractPackCompiler().compile(bad, List.of(), "HEAD"));
        assertEquals("CLOSED_WORLD", e.code());
    }

    @Test
    void extensionCannotRedefineCoreIri(@TempDir Path tmp) throws Exception {
        Path ext = tmp.resolve("extension.ttl");
        Files.writeString(ext, "@prefix erf: <https://ruleops.io/ontology/core#> .\n"
                + "erf:Finding erf:changed true .\n");
        var e = assertThrows(OntologyCompileException.class,
                () -> new OntologyContractPackCompiler().compile(root("core.ttl"), List.of(ext), "HEAD"));
        assertEquals("CORE_REDEFINED", e.code());
    }
}
