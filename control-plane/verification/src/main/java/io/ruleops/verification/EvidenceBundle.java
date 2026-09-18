package io.ruleops.verification;

import java.util.LinkedHashMap;
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
        Map<String, String> environment,
        String notes) {

    public static final String TRUST_LEVEL = "LOCAL_UNATTESTED";

    /** Per-fixture expectation outcome. */
    public record FixtureResult(
            String fixture,
            String category,
            String expectation,
            int expectedHits,
            int actualHits,
            boolean pass) {

        /** Source-compatible constructor for pre-Phase-2 evidence callers. */
        public FixtureResult(String fixture, String expectation, int expectedHits,
                int actualHits, boolean pass) {
            this(fixture, categoryOf(fixture), expectation, expectedHits, actualHits, pass);
        }

        private static String categoryOf(String fixture) {
            int slash = fixture.indexOf('/');
            return slash > 0 ? fixture.substring(0, slash) : "unknown";
        }
    }

    /** Builds the JSON-ready, stably ordered form of the bundle. */
    public Map<String, Object> asJson() {
        var root = new java.util.LinkedHashMap<String, Object>();
        root.put("kind", kind);
        root.put("trustLevel", trustLevel);
        root.put("candidateId", candidateId);
        root.put("candidateDigest", candidateDigest);
        root.put("engine", engine);
        root.put("engineVersion", engineVersion);
        root.put("verdict", verdict);
        root.put("environment", new LinkedHashMap<>(environment));
        root.put("fixtures", fixtures.stream()
                .map(f -> {
                    var json = new java.util.LinkedHashMap<String, Object>();
                    json.put("fixture", f.fixture());
                    json.put("category", f.category());
                    json.put("expectation", f.expectation());
                    json.put("expectedHits", f.expectedHits());
                    json.put("actualHits", f.actualHits());
                    json.put("pass", f.pass());
                    return json;
                })
                .toList());
        root.put("notes", notes);
        return root;
    }
}
