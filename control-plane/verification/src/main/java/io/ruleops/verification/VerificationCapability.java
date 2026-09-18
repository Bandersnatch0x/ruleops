package io.ruleops.verification;

import io.ruleops.factory.FactoryCapability;

/**
 * Verification Lab capability: runs Candidates against fixtures into
 * EvidenceBundle skeletons (the lab itself arrives with the verification
 * tickets). Consumes factory output and owns no authoring or compilation
 * responsibilities (ADR-0018).
 */
public final class VerificationCapability {

    /** Stable identifier of this RuleOps capability (ADR-0018 vocabulary). */
    public static final String NAME = "verification";

    private VerificationCapability() {}

    /** The upstream capability this module is wired to consume. */
    public static String upstreamCapability() {
        return FactoryCapability.NAME;
    }
}
