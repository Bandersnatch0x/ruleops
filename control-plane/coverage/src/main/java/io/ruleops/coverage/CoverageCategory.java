package io.ruleops.coverage;

/**
 * The required classification vocabulary from the Coverage Analysis spec.
 * Categories intentionally retain the cause while {@link #state()} exposes
 * the coarser operational status used by the ledger view.
 */
public enum CoverageCategory {
    EXISTING_RULE(CoverageState.COVERED, true),
    EXISTING_RULE_NOT_ENABLED(CoverageState.PARTIAL, true),
    EXISTING_RULE_UNSUITABLE(CoverageState.PARTIAL, true),
    NEW_RULE_REQUIRED(CoverageState.PARTIAL, false),
    ENVIRONMENT_GAP(CoverageState.BLOCKED, false),
    BACKEND_CAPABILITY_GAP(CoverageState.UNSUPPORTED, false),
    STANDARD_AMBIGUOUS(CoverageState.BLOCKED, false),
    NOT_MACHINE_DETECTABLE(CoverageState.UNSUPPORTED, false);

    private final CoverageState state;
    private final boolean requiresRule;

    CoverageCategory(CoverageState state, boolean requiresRule) {
        this.state = state;
        this.requiresRule = requiresRule;
    }

    public CoverageState state() {
        return state;
    }

    public boolean requiresRule() {
        return requiresRule;
    }
}
