package io.ruleops.coverage;

import java.util.List;

/**
 * Explicit analyst input. A list, rather than a map, preserves duplicate
 * declarations so the analyzer can reject them instead of silently applying
 * last-wins semantics.
 */
public record CoverageInventory(List<ScenarioClassification> classifications) {
    public CoverageInventory {
        if (classifications == null) {
            throw new IllegalArgumentException("classifications are required");
        }
        classifications = List.copyOf(classifications);
    }
}
