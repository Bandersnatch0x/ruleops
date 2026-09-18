package io.ruleops.coverage;

/** One immutable row in a Coverage Ledger. */
public record CoverageEntry(String scenarioId, CoverageCategory category, String ruleId) {
    public CoverageState state() {
        return category.state();
    }
}
