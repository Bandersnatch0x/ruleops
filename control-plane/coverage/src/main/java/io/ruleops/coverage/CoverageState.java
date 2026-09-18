package io.ruleops.coverage;

/** Operational state derived from a single CoverageCategory. */
public enum CoverageState {
    COVERED("covered"),
    PARTIAL("partial"),
    BLOCKED("blocked"),
    UNSUPPORTED("unsupported");

    private final String yamlValue;

    CoverageState(String yamlValue) {
        this.yamlValue = yamlValue;
    }

    public String yamlValue() {
        return yamlValue;
    }
}
