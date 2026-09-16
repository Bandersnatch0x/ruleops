package io.ruleops.authoring;

/**
 * Authoring capability: owned formats and validation for Standard/Scenario,
 * RuleIntent and Rule IR assets (the formats themselves are defined by
 * subsequent authoring tickets). This seam fixes the module boundary first
 * (ADR-0018).
 */
public final class AuthoringCapability {

    /** Stable identifier of this RuleOps capability (ADR-0018 vocabulary). */
    public static final String NAME = "authoring";

    private AuthoringCapability() {}
}
