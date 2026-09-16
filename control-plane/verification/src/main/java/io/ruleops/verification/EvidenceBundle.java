package io.ruleops.verification;

import java.util.List;
import java.util.Map;

/**
 * Evidence for a local verification run over a Candidate's compiled artifact
 * and fixtures. Every result carries the execution identity needed for an
 * independent reviewer to re-run it. The bundle as a whole is marked
 * LOCAL_UNATTESTED (ADR-0085): Contract/Java Local results carry no blocking
 * authority and never become a RuleRelease (ADR-0021).
 */
public record EvidenceBundle(
        String kind,
        String trustLevel,
        String candidateDigest,
        String candidateId,
        String engine,
        String engineVersion,
        String verdict,
        List<FixtureResult> fixtures,
        String notes) {

    public static final String TRUST_LEVEL = "LOCAL_UNATTESTED";

    /** Per-fixture expectation outcome. */
    public record FixtureResult(
            String fixture,
            String expectation,
            int expectedHits,
            int actualHits,
            boolean pass) {}

    /** Builds the JSON-ready ordered form of the bundle. */
    public Map<String, Object> asJson() {
        var root = new java.util.LinkedHashMap<String, Object>();
        root.put("kind", kind);
        root.put("trustLevel", trustLevel);
        root.put("candidateId", candidateId);
        root.put("candidateDigest", candidateDigest);
        root.put("engine", engine);
        root.put("engineVersion", engineVersion);
        root.put("verdict", verdict);
        root.put("fixtures", fixtures.stream()
                .map(f -> Map.<String, Object>of(
                        "fixture", f.fixture(),
                        "expectation", f.expectation(),
                        "expectedHits", f.expectedHits(),
                        "actualHits", f.actualHits(),
                        "pass", f.pass()))
                .toList());
        root.put("notes", notes);
        return root;
    }
}
