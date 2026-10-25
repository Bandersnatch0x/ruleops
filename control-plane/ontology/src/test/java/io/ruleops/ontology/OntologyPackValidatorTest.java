package io.ruleops.ontology;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.List;
import org.junit.jupiter.api.Test;

class OntologyPackValidatorTest {
    private static final Path VECTOR = Path.of("..", "..", "specs", "ontology", "vectors", "core-java-v0.nt");
    private static final Path WIRE_VECTOR = Path.of("..", "..", "specs", "ontology", "vectors", "rdfc-pack-v1.txt");
    private static final String CANONICAL_DIGEST = "146149a0cb35d92e012d2b05e1cdbe796a4779f40eddc666fa10353fabe1d905";
    private static final String SHAPES_DIGEST = "e729e8617f159df10206c3b34040472573ea5efbea35f9bda7ac4bb967ce81f6";
    private static final String DEPENDENCY_DIGEST = "e2ee4760b88e1e3581435a4a1b09720de0c5d540c3b1b9c8f8c0b7d8f8a292de";
    private static final String ARTIFACT_DIGEST = "6d0b4b40a2ef3dd266eaf28c589b727c74783a10784a8a2085c0c8d54b5ba421";

    private static OntologyPackEnvelope validPack() throws Exception {
        byte[] canonical = Files.readAllBytes(VECTOR);
        assertEquals(CANONICAL_DIGEST, HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(canonical)));
        return new OntologyPackEnvelope(
                "rdfc-v0-core-java", canonical, CANONICAL_DIGEST, SHAPES_DIGEST,
                List.of(DEPENDENCY_DIGEST), "https://ruleops.io/ontology/core#",
                "jena-5.3.0", ARTIFACT_DIGEST, "ontology-signing", signature());
    }

    @Test
    void validEnvelopeRoundTripsDeterministically() throws Exception {
        OntologyPackValidator validator = new OntologyPackValidator();
        OntologyPackEnvelope original = validPack();
        assertTrue(validator.validate(original).valid());
        byte[] wire = validator.serialize(original);
        OntologyPackEnvelope restored = validator.parse(wire);
        assertEquals(original.revision(), restored.revision());
        assertEquals(original.canonicalDigest(), restored.canonicalDigest());
        assertEquals(original.dependencyDigests(), restored.dependencyDigests());
        assertArrayEquals(original.canonicalBytes(), restored.canonicalBytes());
        assertArrayEquals(wire, validator.serialize(restored));
    }

    @Test
    void checkedInWireVectorParsesAndSerializesByteIdentically() throws Exception {
        OntologyPackValidator validator = new OntologyPackValidator();
        byte[] expected = Files.readAllBytes(WIRE_VECTOR);
        OntologyPackEnvelope pack = validator.parse(expected);
        assertTrue(validator.validate(pack).valid());
        assertArrayEquals(expected, validator.serialize(pack));
        assertEquals("example-v1", pack.revision());
    }

    @Test
    void canonicalDigestMismatchIsRejected() throws Exception {
        OntologyPackEnvelope valid = validPack();
        OntologyPackEnvelope tampered = new OntologyPackEnvelope(
                valid.revision(), valid.canonicalBytes(), "f".repeat(64), valid.shapesDigest(),
                valid.dependencyDigests(), valid.namespace(), valid.toolchainIdentity(),
                valid.artifactDigest(), valid.signingRole(), valid.signature());
        var result = new OntologyPackValidator().validate(tampered);
        assertFalse(result.valid());
        assertEquals("CANONICAL_DIGEST_MISMATCH", result.code());
    }

    @Test
    void dependenciesMustBeSortedAndUnique() throws Exception {
        OntologyPackEnvelope valid = validPack();
        OntologyPackEnvelope unsorted = new OntologyPackEnvelope(
                valid.revision(), valid.canonicalBytes(), valid.canonicalDigest(), valid.shapesDigest(),
                List.of("f".repeat(64), "1".repeat(64)), valid.namespace(), valid.toolchainIdentity(),
                valid.artifactDigest(), valid.signingRole(), valid.signature());
        var result = new OntologyPackValidator().validate(unsorted);
        assertFalse(result.valid());
        assertEquals("NON_CANONICAL_DEPENDENCIES", result.code());
    }

    @Test
    void placeholderDigestIsRejected() throws Exception {
        OntologyPackEnvelope valid = validPack();
        OntologyPackEnvelope placeholder = new OntologyPackEnvelope(
                valid.revision(), valid.canonicalBytes(), valid.canonicalDigest(), "0".repeat(64),
                valid.dependencyDigests(), valid.namespace(), valid.toolchainIdentity(),
                valid.artifactDigest(), valid.signingRole(), valid.signature());
        var result = new OntologyPackValidator().validate(placeholder);
        assertFalse(result.valid());
        assertEquals("INVALID_DIGEST", result.code());
    }

    @Test
    void placeholderSignatureIsRejected() throws Exception {
        OntologyPackEnvelope valid = validPack();
        OntologyPackEnvelope placeholder = new OntologyPackEnvelope(
                valid.revision(), valid.canonicalBytes(), valid.canonicalDigest(), valid.shapesDigest(),
                valid.dependencyDigests(), valid.namespace(), valid.toolchainIdentity(),
                valid.artifactDigest(), valid.signingRole(), new byte[32]);
        var result = new OntologyPackValidator().validate(placeholder);
        assertFalse(result.valid());
        assertEquals("SIGNATURE_MISSING", result.code());
    }

    @Test
    void productionAdmissionRemainsFailClosed() throws Exception {
        var decision = new RdfcAdmission().evaluate(validPack());
        assertFalse(decision.admitted());
        assertEquals(RdfcAdmissionStatus.RDFC_NOT_ADMITTED, decision.status());
        assertTrue(decision.validation().valid());
    }

    @Test
    void malformedWireFailsClosed() {
        var validator = new OntologyPackValidator();
        var error = assertThrows(OntologyPackValidationException.class,
                () -> validator.parse("rdfc-pack-v1\nrevision=bad\n".getBytes(StandardCharsets.UTF_8)));
        assertEquals("INVALID_FORMAT", error.code());
    }

    @Test
    void alternateBase64PaddingFailsDeterministicEncodingCheck() throws Exception {
        OntologyPackValidator validator = new OntologyPackValidator();
        byte[] wire = validator.serialize(validPack());
        String padded = new String(wire, StandardCharsets.UTF_8).replaceFirst(
                "revision=([A-Za-z0-9_-]+)", "revision=$1=");
        var error = assertThrows(OntologyPackValidationException.class,
                () -> validator.parse(padded.getBytes(StandardCharsets.UTF_8)));
        assertEquals("NON_CANONICAL_ENCODING", error.code());
    }

    private static byte[] signature() {
        byte[] value = new byte[32];
        java.util.Arrays.fill(value, (byte) 0xab);
        return value;
    }
}
