package io.ruleops.ontology;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.HexFormat;

/** Immutable, content-addressed v0 ontology pack. */
public record OntologyContractPack(String revision, String digest, byte[] canonicalTriples) {
    public OntologyContractPack {
        canonicalTriples = canonicalTriples.clone();
    }
    public byte[] canonicalTriples() { return canonicalTriples.clone(); }
    public String identity() { return "ontology-pack-v0:" + digest; }
    static String sha256(byte[] bytes) {
        try { return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes)); }
        catch (Exception e) { throw new IllegalStateException("SHA-256 unavailable", e); }
    }
}
