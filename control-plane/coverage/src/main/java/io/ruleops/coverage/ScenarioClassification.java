package io.ruleops.coverage;

/** One explicit classification for one Standard Scenario. */
public record ScenarioClassification(String scenarioId, CoverageCategory category, String ruleId) {
    public ScenarioClassification {
        if (scenarioId == null || scenarioId.isBlank()) {
            throw new IllegalArgumentException("scenarioId must be non-blank");
        }
        if (category == null) {
            throw new IllegalArgumentException("category is required");
        }
        ruleId = ruleId == null ? "" : ruleId;
    }
}
