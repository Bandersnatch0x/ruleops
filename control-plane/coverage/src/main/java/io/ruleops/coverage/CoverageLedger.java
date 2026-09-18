package io.ruleops.coverage;

import io.ruleops.authoring.model.Standard;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.List;

/**
 * Durable, deterministic Coverage Ledger for one Standard revision. It is
 * deliberately independent from RuleRelease and Gate authority.
 */
public record CoverageLedger(String standardId, String standardVersion, List<CoverageEntry> entries) {
    public CoverageLedger {
        if (standardId == null || standardId.isBlank() || standardVersion == null || standardVersion.isBlank()) {
            throw new IllegalArgumentException("standard identity is required");
        }
        var sorted = new ArrayList<>(entries == null ? List.<CoverageEntry>of() : entries);
        sorted.sort(java.util.Comparator.comparing(CoverageEntry::scenarioId));
        entries = List.copyOf(sorted);
    }

    /** Stable inspection/persistence form; IDs are validated before emission. */
    public String asYaml() {
        var out = new StringBuilder();
        out.append("apiVersion: rulefactory.io/v1\n");
        out.append("kind: CoverageLedger\n");
        out.append("standard:\n");
        out.append("  id: ").append(standardId).append('\n');
        out.append("  version: ").append(standardVersion).append('\n');
        out.append("coverage:\n");
        for (CoverageEntry entry : entries) {
            out.append("  ").append(entry.scenarioId()).append(":\n");
            out.append("    category: ").append(entry.category()).append('\n');
            out.append("    status: ").append(entry.state().yamlValue()).append('\n');
            out.append("    rule: ").append(entry.ruleId().isBlank() ? "null" : entry.ruleId()).append('\n');
        }
        return out.toString();
    }

    public String digest() {
        try {
            return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256")
                    .digest(asYaml().getBytes(StandardCharsets.UTF_8)));
        } catch (NoSuchAlgorithmException e) {
            throw new AssertionError("JDK must provide SHA-256", e);
        }
    }
}
