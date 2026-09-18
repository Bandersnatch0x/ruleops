package io.ruleops.factory;

import io.ruleops.authoring.AuthoringCapability;

/**
 * Factory Kernel capability: compiles governed authoring assets into
 * verification-ready Candidates (compilation itself arrives with the factory
 * tickets). Consumes the authoring capability; consumed by verification
 * (ADR-0018).
 */
public final class FactoryCapability {

    /** Stable identifier of this RuleOps capability (ADR-0018 vocabulary). */
    public static final String NAME = "factory";

    private FactoryCapability() {}

    /** The upstream capability this module is wired to consume. */
    public static String upstreamCapability() {
        return AuthoringCapability.NAME;
    }
}
