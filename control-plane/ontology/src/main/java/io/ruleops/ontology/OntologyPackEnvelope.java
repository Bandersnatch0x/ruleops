package io.ruleops.ontology;

import java.util.Arrays;
import java.util.List;
import java.util.Objects;

/**
 * The metadata envelope for the ADR-0181 deterministic pack slice.
 *
 * <p>This type deliberately contains no RDF or Jena types.  It is a transport
 * value only; {@link OntologyPackValidator} performs all structural and digest
 * checks before a pack can be serialized or inspected.</p>
 */
public record OntologyPackEnvelope(
        String revision,
        byte[] canonicalBytes,
        String canonicalDigest,
        String shapesDigest,
        List<String> dependencyDigests,
        String namespace,
        String toolchainIdentity,
        String artifactDigest,
        String signingRole,
        byte[] signature) {

    public OntologyPackEnvelope {
        Objects.requireNonNull(revision, "revision");
        Objects.requireNonNull(canonicalBytes, "canonicalBytes");
        Objects.requireNonNull(canonicalDigest, "canonicalDigest");
        Objects.requireNonNull(shapesDigest, "shapesDigest");
        Objects.requireNonNull(dependencyDigests, "dependencyDigests");
        Objects.requireNonNull(namespace, "namespace");
        Objects.requireNonNull(toolchainIdentity, "toolchainIdentity");
        Objects.requireNonNull(artifactDigest, "artifactDigest");
        Objects.requireNonNull(signingRole, "signingRole");
        Objects.requireNonNull(signature, "signature");
        canonicalBytes = canonicalBytes.clone();
        dependencyDigests = List.copyOf(dependencyDigests);
        signature = signature.clone();
    }

    @Override
    public byte[] canonicalBytes() {
        return canonicalBytes.clone();
    }

    @Override
    public byte[] signature() {
        return signature.clone();
    }

    /** Content identity used by registry references; revision is not identity. */
    public String identity() {
        return OntologyPackValidator.FORMAT + ":" + canonicalDigest;
    }

    /** Return a defensive copy useful to callers comparing test vectors. */
    public boolean sameBytes(OntologyPackEnvelope other) {
        return other != null && Arrays.equals(canonicalBytes, other.canonicalBytes);
    }
}
