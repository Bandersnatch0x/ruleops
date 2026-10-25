package io.ruleops.ontology;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Base64;
import java.util.HexFormat;
import java.util.List;
import java.util.regex.Pattern;

/**
 * Validates and serializes the small, language-neutral ADR-0181 pack envelope.
 *
 * <p>The envelope is intentionally not an RDFC implementation and this class
 * does not verify a cryptographic signature.  It only freezes a deterministic
 * representation and checks that all digests and required admission metadata
 * are internally consistent.  Production admission remains fail-closed in
 * {@link RdfcAdmission}.</p>
 */
public final class OntologyPackValidator {
    public static final String FORMAT = "rdfc-pack-v1";
    public static final String REQUIRED_SIGNING_ROLE = "ontology-signing";
    public static final int MAX_CANONICAL_BYTES = 8 * 1024 * 1024;
    public static final int MAX_WIRE_BYTES = 16 * 1024 * 1024;
    public static final int MAX_DEPENDENCIES = 128;

    private static final Pattern SHA256 = Pattern.compile("[0-9a-f]{64}");
    private static final Base64.Encoder B64_ENCODER = Base64.getUrlEncoder().withoutPadding();
    private static final Base64.Decoder B64_DECODER = Base64.getUrlDecoder();

    /** Result returned instead of silently accepting malformed metadata. */
    public record ValidationResult(boolean valid, String code, String message) {
        public static ValidationResult validResult() {
            return new ValidationResult(true, "VALID", "ok");
        }

        public static ValidationResult invalid(String code, String message) {
            return new ValidationResult(false, code, message);
        }
    }

    /** Validate all fields, canonical bytes and dependency ordering. */
    public ValidationResult validate(OntologyPackEnvelope pack) {
        if (pack == null) {
            return ValidationResult.invalid("NULL_PACK", "pack is required");
        }
        if (pack.canonicalBytes().length > MAX_CANONICAL_BYTES) {
            return ValidationResult.invalid("RESOURCE_LIMIT", "canonical bytes exceed limit");
        }
        if (pack.revision().isBlank() || pack.revision().getBytes(StandardCharsets.UTF_8).length > 512) {
            return ValidationResult.invalid("INVALID_REVISION", "revision is empty or too long");
        }
        if (pack.namespace().isBlank() || pack.namespace().getBytes(StandardCharsets.UTF_8).length > 512) {
            return ValidationResult.invalid("INVALID_NAMESPACE", "namespace is empty or too long");
        }
        if (pack.toolchainIdentity().isBlank() || pack.toolchainIdentity().length() > 512) {
            return ValidationResult.invalid("INVALID_TOOLCHAIN", "toolchain identity is empty or too long");
        }
        if (!REQUIRED_SIGNING_ROLE.equals(pack.signingRole())) {
            return ValidationResult.invalid("SIGNING_ROLE_REQUIRED", "dedicated ontology signing role is required");
        }
        if (!validDigest(pack.canonicalDigest()) || !validDigest(pack.shapesDigest())
                || !validDigest(pack.artifactDigest())) {
            return ValidationResult.invalid("INVALID_DIGEST", "all digests must be lowercase SHA-256 hex");
        }
        if (!MessageDigest.isEqual(hex(pack.canonicalBytes()), decodeHex(pack.canonicalDigest()))) {
            return ValidationResult.invalid("CANONICAL_DIGEST_MISMATCH", "canonical digest does not match bytes");
        }
        if (pack.signature().length < 32) {
            return ValidationResult.invalid("SIGNATURE_MISSING", "signature bytes are required");
        }
        List<String> dependencies = pack.dependencyDigests();
        if (dependencies.size() > MAX_DEPENDENCIES) {
            return ValidationResult.invalid("RESOURCE_LIMIT", "too many dependency digests");
        }
        String previous = null;
        for (String dependency : dependencies) {
            if (!validDigest(dependency)) {
                return ValidationResult.invalid("INVALID_DEPENDENCY_DIGEST", "dependency digest is not SHA-256 hex");
            }
            if (previous != null && previous.compareTo(dependency) >= 0) {
                return ValidationResult.invalid("NON_CANONICAL_DEPENDENCIES",
                        "dependency digests must be sorted and unique");
            }
            previous = dependency;
        }
        return ValidationResult.validResult();
    }

    /** Serialize a validated envelope in fixed field order. */
    public byte[] serialize(OntologyPackEnvelope pack) throws OntologyPackValidationException {
        ValidationResult result = validate(pack);
        if (!result.valid()) {
            throw failure(result);
        }
        String dependencies = String.join(",", pack.dependencyDigests());
        String wire = String.join("\n",
                FORMAT,
                "revision=" + encode(pack.revision()),
                "canonical_digest=" + pack.canonicalDigest(),
                "canonical_length=" + pack.canonicalBytes().length,
                "shapes_digest=" + pack.shapesDigest(),
                "dependency_digests=" + dependencies,
                "namespace=" + encode(pack.namespace()),
                "toolchain=" + encode(pack.toolchainIdentity()),
                "artifact_digest=" + pack.artifactDigest(),
                "signing_role=" + encode(pack.signingRole()),
                "signature=" + encode(pack.signature()),
                "canonical_bytes=" + encode(pack.canonicalBytes()),
                "");
        byte[] bytes = wire.getBytes(StandardCharsets.UTF_8);
        if (bytes.length > MAX_WIRE_BYTES) {
            throw new OntologyPackValidationException("RESOURCE_LIMIT", "wire bytes exceed limit");
        }
        return bytes;
    }

    /** Parse and validate the fixed-field wire format. */
    public OntologyPackEnvelope parse(byte[] wire) throws OntologyPackValidationException {
        if (wire == null || wire.length == 0) {
            throw new OntologyPackValidationException("INVALID_FORMAT", "wire bytes are required");
        }
        if (wire.length > MAX_WIRE_BYTES) {
            throw new OntologyPackValidationException("RESOURCE_LIMIT", "wire bytes exceed limit");
        }
        String text = new String(wire, StandardCharsets.UTF_8);
        String[] lines = text.split("\\n", -1);
        if (lines.length != 13 || !FORMAT.equals(lines[0]) || !lines[12].isEmpty()) {
            throw new OntologyPackValidationException("INVALID_FORMAT", "unexpected wire field count or version");
        }
        try {
            String revision = value(lines[1], "revision");
            String canonicalDigest = value(lines[2], "canonical_digest");
            int canonicalLength = Integer.parseInt(value(lines[3], "canonical_length"));
            String shapesDigest = value(lines[4], "shapes_digest");
            String dependencyText = value(lines[5], "dependency_digests");
            List<String> dependencies = new ArrayList<>();
            if (!dependencyText.isEmpty()) {
                dependencies.addAll(List.of(dependencyText.split(",", -1)));
            }
            String namespace = decodeText(value(lines[6], "namespace"));
            String toolchain = decodeText(value(lines[7], "toolchain"));
            String artifactDigest = value(lines[8], "artifact_digest");
            String signingRole = decodeText(value(lines[9], "signing_role"));
            byte[] signature = decode(value(lines[10], "signature"));
            byte[] canonicalBytes = decode(value(lines[11], "canonical_bytes"));
            if (canonicalLength != canonicalBytes.length) {
                throw new OntologyPackValidationException("CANONICAL_LENGTH_MISMATCH",
                        "canonical length does not match bytes");
            }
            OntologyPackEnvelope pack = new OntologyPackEnvelope(revision, canonicalBytes, canonicalDigest,
                    shapesDigest, dependencies, namespace, toolchain, artifactDigest, signingRole, signature);
            ValidationResult result = validate(pack);
            if (!result.valid()) {
                throw failure(result);
            }
            return pack;
        } catch (NumberFormatException e) {
            throw new OntologyPackValidationException("INVALID_FORMAT", "canonical length is not an integer", e);
        } catch (IllegalArgumentException e) {
            throw new OntologyPackValidationException("INVALID_ENCODING", "base64 field is invalid", e);
        }
    }

    private static String value(String line, String field) throws OntologyPackValidationException {
        String prefix = field + "=";
        if (!line.startsWith(prefix)) {
            throw new OntologyPackValidationException("INVALID_FORMAT", "expected field " + field);
        }
        return line.substring(prefix.length());
    }

    private static String encode(String value) {
        return B64_ENCODER.encodeToString(value.getBytes(StandardCharsets.UTF_8));
    }

    private static String encode(byte[] value) {
        return B64_ENCODER.encodeToString(value);
    }

    private static String decodeText(String value) {
        return new String(decode(value), StandardCharsets.UTF_8);
    }

    private static byte[] decode(String value) {
        return B64_DECODER.decode(value);
    }

    private static boolean validDigest(String digest) {
        return digest != null && SHA256.matcher(digest).matches() && !digest.chars().allMatch(c -> c == '0');
    }

    private static byte[] hex(byte[] value) {
        try {
            return MessageDigest.getInstance("SHA-256").digest(value);
        } catch (java.security.NoSuchAlgorithmException e) {
            throw new IllegalStateException("SHA-256 unavailable", e);
        }
    }

    private static byte[] decodeHex(String value) {
        return HexFormat.of().parseHex(value);
    }

    private static OntologyPackValidationException failure(ValidationResult result) {
        return new OntologyPackValidationException(result.code(), result.message());
    }
}
